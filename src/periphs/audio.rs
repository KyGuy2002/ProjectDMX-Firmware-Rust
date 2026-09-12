use defmt::println;

use embassy_futures::join::join;
use embassy_futures::yield_now;
use embassy_rp::pac;
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::i2s::{PioI2sOut, PioI2sOutProgram};
use embassy_time::Instant;
use nanomp3::{Decoder, MAX_SAMPLES_PER_FRAME};

use crate::config::AudioConfig;
use crate::hardware::{AudioIrqs, AudioResources};
use crate::periphs::sd_stream::{StreamEvent, VoiceChannel, SD_CHUNK_SIZE};
use crate::read_channels;

// All source MP3s are expected at this rate - no resampling is done.
const AUDIO_SAMPLE_RATE: u32 = 44100;

// Playback volume, 0.0 - 1.0. Tweak here.
const VOLUME: f32 = 0.1;

// nanomp3 decodes interleaved; MAX_SAMPLES_PER_FRAME counts individual samples,
// so a mono frame is at most half that many.
const MAX_FRAME_SAMPLES: usize = MAX_SAMPLES_PER_FRAME / 2;

// Per-voice reassembly buffer for chunks arriving from core1's SD reader.
// Sized with headroom for one full incoming chunk past the point decode gives
// up and asks for more, so a chunk can never overflow it (see the `+
// SD_CHUNK_SIZE` guard in `decode_next_frame`).
const VOICE_MP3_BUF_SIZE: usize = 4 * 1024;

// Frames per I2S DMA transfer / buffer. While one buffer plays (~FRAMES_PER_BATCH
// * 26 ms of DMA) the other is refilled, so this is the headroom `fill()` has to
// decode the next batch and service any SD read latency spike. 8 frames = ~209 ms
// per buffer, comfortably above the worst-case decode + SD stall. (Startup delay
// to first audio is dominated by the MP3s' own silent lead-in, not this.)
const FRAMES_PER_BATCH: usize = 8;
const OUT_BUF_LEN: usize = MAX_FRAME_SAMPLES * FRAMES_PER_BATCH;

// PIO1 SM0 drives the I2S output. Its FDEBUG.TXSTALL bit latches whenever the
// state machine runs the TX FIFO dry waiting for the next DMA word - i.e. an
// audio underrun / audible glitch. Reading + clearing it each buffer turns
// "sounds like it stutters sometimes" into a hard count.
const I2S_SM_MASK: u8 = 0b0001;

fn i2s_underran() -> bool {
    pac::PIO1.fdebug().read().txstall() & I2S_SM_MASK != 0
}

fn clear_i2s_underrun() {
    pac::PIO1.fdebug().write(|w| w.set_txstall(I2S_SM_MASK));
}

#[derive(Clone, Copy, PartialEq)]
enum PlaybackMode {
    Once,
    Loop,
    Both,
}

/// Maps a DMX value to `(file_index, mode)`:
/// - `0` => `None` (stop)
/// - `1..=85` => `(v - 1, Once)`
/// - `86..=170` => `(v - 86, Loop)`
/// - `171..=255` => `(v - 171, Both)` (route to both outputs)
/// - resolved index past the end of the list => `None` (stop)
fn decode_value(v: u8, num_files: usize) -> Option<(usize, PlaybackMode)> {
    if v == 0 {
        return None;
    }

    let (idx, mode) = if v <= 85 {
        ((v - 1) as usize, PlaybackMode::Once)
    } else if v <= 170 {
        ((v - 86) as usize, PlaybackMode::Loop)
    } else {
        ((v - 171) as usize, PlaybackMode::Both)
    };

    (idx < num_files).then_some((idx, mode))
}

struct Voice {
    file_index: usize,
    mode: PlaybackMode,
    // Set once a one-shot plays through to EOF, or its file failed to open.
    // The voice is kept (silent) rather than dropped, so reconcile() doesn't
    // see "nothing playing" and restart it every fill. Cleared only by
    // selecting a different file (or 0).
    finished: bool,
    open_failed: bool,
    // True once core1 has reported real end-of-file for this voice's current
    // selection (never set for Loop mode - core1 rewinds transparently).
    eof_seen: bool,

    // Identifies which selection this voice's channel events belong to, so a
    // stale event from a since-replaced selection can be told apart and
    // discarded instead of being decoded as if it were fresh data.
    generation: u32,
    channel: &'static VoiceChannel,
    chunks_received: u32,

    decoder: Decoder,
    mp3_buf: [u8; VOICE_MP3_BUF_SIZE],
    buf_len: usize,

    // Mono samples decoded but not yet copied into the output buffer.
    carry: [f32; MAX_FRAME_SAMPLES],
    carry_len: usize,
    carry_pos: usize,
}

impl Voice {
    fn start(
        channel: &'static VoiceChannel,
        next_gen: &mut u32,
        file_index: usize,
        mode: PlaybackMode,
        filename: &heapless::String<{ crate::config::MAX_FILENAME_LEN }>,
    ) -> Voice {
        *next_gen = next_gen.wrapping_add(1);
        let generation = *next_gen;

        println!(
            "Audio: starting voice file_index={} mode={} generation={}",
            file_index,
            match mode {
                PlaybackMode::Once => "once",
                PlaybackMode::Loop => "loop",
                PlaybackMode::Both => "both",
            },
            generation
        );

        channel
            .selection
            .signal(Some(crate::periphs::sd_stream::StreamSelection {
                filename: filename.clone(),
                loop_playback: mode == PlaybackMode::Loop,
                generation,
            }));

        Voice {
            file_index,
            mode,
            finished: false,
            open_failed: false,
            eof_seen: false,
            generation,
            channel,
            chunks_received: 0,
            decoder: Decoder::new(),
            mp3_buf: [0u8; VOICE_MP3_BUF_SIZE],
            buf_len: 0,
            carry: [0f32; MAX_FRAME_SAMPLES],
            carry_len: 0,
            carry_pos: 0,
        }
    }

    /// Waits for the next update from core1's SD reader and applies it.
    /// Discards events left over from a since-replaced selection (mismatched
    /// `generation`) instead of treating them as fresh data.
    async fn pull_more(&mut self) {
        loop {
            match self.channel.events.receive().await {
                StreamEvent::Data { generation, len, buf } => {
                    if generation != self.generation {
                        continue;
                    }
                    self.chunks_received += 1;
                    if self.chunks_received == 1 || self.chunks_received % 100 == 0 {
                        println!(
                            "Audio: generation={} received chunk #{} ({} bytes)",
                            generation, self.chunks_received, len
                        );
                    }
                    let n = len as usize;
                    self.mp3_buf[self.buf_len..self.buf_len + n].copy_from_slice(&buf[..n]);
                    self.buf_len += n;
                    return;
                }
                StreamEvent::Eof { generation } => {
                    if generation != self.generation {
                        continue;
                    }
                    println!(
                        "Audio: generation={} got EOF after {} chunks",
                        generation, self.chunks_received
                    );
                    self.eof_seen = true;
                    return;
                }
                StreamEvent::OpenFailed { generation } => {
                    if generation != self.generation {
                        continue;
                    }
                    println!("Audio: generation={} failed to open", generation);
                    self.open_failed = true;
                    self.eof_seen = true;
                    return;
                }
            }
        }
    }

    /// Decodes the next MP3 frame into `self.carry` as mono (downmixing a
    /// stereo source). Returns `false` once there is genuinely no more audio
    /// (one-shot EOF, or the file never opened).
    async fn decode_next_frame(&mut self, scratch: &mut [f32; MAX_SAMPLES_PER_FRAME]) -> bool {
        loop {
            if self.buf_len == 0 {
                if self.eof_seen {
                    return false;
                }
                self.pull_more().await;
                continue;
            }

            let (mut consumed, info) = self.decoder.decode(&self.mp3_buf[..self.buf_len], scratch);

            if consumed == 0 && info.is_none() {
                // Leave room for one full incoming chunk before giving up on
                // pulling more - see VOICE_MP3_BUF_SIZE's comment.
                if self.eof_seen || self.buf_len + SD_CHUNK_SIZE > VOICE_MP3_BUF_SIZE {
                    // A frame header sits at offset 0 but the frame isn't complete
                    // and no more data is coming (or the buffer's genuinely full) -
                    // skip to the next sync candidate (0xFF followed by 0xE_/0xF_)
                    // in one move rather than nudging one byte at a time.
                    let mut skip = 1;
                    while skip + 1 < self.buf_len
                        && !(self.mp3_buf[skip] == 0xFF && (self.mp3_buf[skip + 1] & 0xE0) == 0xE0)
                    {
                        skip += 1;
                    }
                    consumed = skip;
                } else {
                    self.pull_more().await;
                    continue;
                }
            }

            self.mp3_buf.copy_within(consumed..self.buf_len, 0);
            self.buf_len -= consumed;

            if let Some(info) = info {
                let channels = info.channels.num() as usize;
                let n = info.samples_produced;

                if channels > 1 {
                    for i in 0..n {
                        self.carry[i] = (scratch[i * 2] + scratch[i * 2 + 1]) * 0.5;
                    }
                } else {
                    self.carry[..n].copy_from_slice(&scratch[..n]);
                }

                self.carry_len = n;
                self.carry_pos = 0;
                return true;
            }
        }
    }

    /// Fills `out[..count]` with mono samples. Returns how many were actually
    /// written - `< count` (or 0) once a one-shot has ended; the caller zero-fills
    /// the remainder.
    async fn produce(
        &mut self,
        out: &mut [f32],
        count: usize,
        scratch: &mut [f32; MAX_SAMPLES_PER_FRAME],
    ) -> usize {
        if self.finished {
            return 0;
        }

        for i in 0..count {
            if self.carry_pos >= self.carry_len && !self.decode_next_frame(scratch).await {
                self.finished = true;
                return i;
            }

            out[i] = self.carry[self.carry_pos];
            self.carry_pos += 1;
        }

        count
    }
}

/// Brings `voice` in line with the current DMX value. Keeps the voice untouched
/// when it already matches `(index, looping)` (so a finished one-shot stays
/// silent); otherwise starts streaming the new file, or clears the voice on
/// `0` / an out-of-range selection.
fn reconcile(
    voice: &mut Option<Voice>,
    failed_selection: &mut Option<(usize, PlaybackMode)>,
    channel: &'static VoiceChannel,
    next_gen: &mut u32,
    files: &[heapless::String<{ crate::config::MAX_FILENAME_LEN }>],
    dmx: u8,
) {
    match decode_value(dmx, files.len()) {
        None => {
            if voice.is_some() {
                channel.selection.signal(None);
            }
            *voice = None;
            *failed_selection = None;
        }
        Some((idx, mode)) => {
            let matches = voice
                .as_ref()
                .is_some_and(|v| v.file_index == idx && v.mode == mode && !v.open_failed);

            if matches {
                *failed_selection = None;
            } else if *failed_selection != Some((idx, mode)) {
                *voice = Some(Voice::start(channel, next_gen, idx, mode, &files[idx]));
            }
        }
    }

    // Once a started voice's open failure comes back (asynchronously, from
    // core1), remember it so reconcile() doesn't immediately retry the same
    // selection every fill() tick.
    if let Some(v) = voice.as_ref() {
        if v.open_failed {
            *failed_selection = Some((v.file_index, v.mode));
        }
    }
}

/// Reads both DMX channels, reconciles each voice independently, and renders a
/// full stereo `out` buffer. Yields once per frame.
#[allow(clippy::too_many_arguments)]
async fn fill(
    cfg: &AudioConfig,
    left_channel: &'static VoiceChannel,
    right_channel: &'static VoiceChannel,
    left_gen: &mut u32,
    right_gen: &mut u32,
    left_voice: &mut Option<Voice>,
    right_voice: &mut Option<Voice>,
    left_failed_selection: &mut Option<(usize, PlaybackMode)>,
    right_failed_selection: &mut Option<(usize, PlaybackMode)>,
    scratch: &mut [f32; MAX_SAMPLES_PER_FRAME],
    out: &mut [u32; OUT_BUF_LEN],
) {
    let channels = read_channels::<2>(cfg.universe as usize, cfg.start_channel as usize);
    reconcile(
        left_voice,
        left_failed_selection,
        left_channel,
        left_gen,
        &cfg.left_files,
        channels[0],
    );
    reconcile(
        right_voice,
        right_failed_selection,
        right_channel,
        right_gen,
        &cfg.right_files,
        channels[1],
    );

    let left_shared = left_voice
        .as_ref()
        .is_some_and(|voice| voice.mode == PlaybackMode::Both);
    let right_shared = right_voice
        .as_ref()
        .is_some_and(|voice| voice.mode == PlaybackMode::Both);

    let mut pos = 0;
    while pos + MAX_FRAME_SAMPLES <= OUT_BUF_LEN {
        let mut left = [0f32; MAX_FRAME_SAMPLES];
        let mut right = [0f32; MAX_FRAME_SAMPLES];

        let left_produced = match left_voice {
            Some(v) => v.produce(&mut left, MAX_FRAME_SAMPLES, scratch).await,
            None => 0,
        };
        for sample in &mut left[left_produced..] {
            *sample = 0.0;
        }

        let right_produced = match right_voice {
            Some(v) => v.produce(&mut right, MAX_FRAME_SAMPLES, scratch).await,
            None => 0,
        };
        for sample in &mut right[right_produced..] {
            *sample = 0.0;
        }

        for i in 0..MAX_FRAME_SAMPLES {
            let left_sample = left[i];
            let right_sample = right[i];
            if left_shared {
                right[i] += left_sample;
            }
            if right_shared {
                left[i] += right_sample;
            }

            let left_s16 =
                (left[i].clamp(-1.0, 1.0) * VOLUME * 32767.0) as i32 as i16 as u16;
            let right_s16 =
                (right[i].clamp(-1.0, 1.0) * VOLUME * 32767.0) as i32 as i16 as u16;
            out[pos + i] = ((left_s16 as u32) << 16) | (right_s16 as u32);
        }

        pos += MAX_FRAME_SAMPLES;
        yield_now().await;
    }
}

#[embassy_executor::task]
pub async fn audio_task(
    cfg: AudioConfig,
    r: AudioResources,
    left_channel: &'static VoiceChannel,
    right_channel: &'static VoiceChannel,
) {
    println!("Audio task started.");

    let Pio { mut common, sm0, .. } = Pio::new(r.pio, AudioIrqs);
    let i2s_program = PioI2sOutProgram::new(&mut common);

    let mut i2s = PioI2sOut::new(
        &mut common,
        sm0,
        r.dma,
        r.din,
        r.bck,
        r.lck,
        AUDIO_SAMPLE_RATE,
        16,
        &i2s_program,
    );

    let mut left_voice: Option<Voice> = None;
    let mut right_voice: Option<Voice> = None;
    let mut left_failed_selection: Option<(usize, PlaybackMode)> = None;
    let mut right_failed_selection: Option<(usize, PlaybackMode)> = None;
    let mut left_gen: u32 = 0;
    let mut right_gen: u32 = 0;
    let mut scratch = [0f32; MAX_SAMPLES_PER_FRAME];
    let mut buf_a = [0u32; OUT_BUF_LEN];
    let mut buf_b = [0u32; OUT_BUF_LEN];

    fill(
        &cfg,
        left_channel,
        right_channel,
        &mut left_gen,
        &mut right_gen,
        &mut left_voice,
        &mut right_voice,
        &mut left_failed_selection,
        &mut right_failed_selection,
        &mut scratch,
        &mut buf_a,
    )
    .await;

    // The SM stalls continuously until the first DMA transfer feeds the FIFO;
    // clear that expected startup latch so the counter only sees real underruns.
    clear_i2s_underrun();
    let mut underruns: u32 = 0;
    let mut reported: u32 = 0;
    let mut last_report = Instant::now();

    loop {
        join(
            i2s.write(&buf_a[..]),
            fill(
                &cfg,
                left_channel,
                right_channel,
                &mut left_gen,
                &mut right_gen,
                &mut left_voice,
                &mut right_voice,
                &mut left_failed_selection,
                &mut right_failed_selection,
                &mut scratch,
                &mut buf_b,
            ),
        )
        .await;
        if i2s_underran() {
            underruns += 1;
            clear_i2s_underrun();
        }

        join(
            i2s.write(&buf_b[..]),
            fill(
                &cfg,
                left_channel,
                right_channel,
                &mut left_gen,
                &mut right_gen,
                &mut left_voice,
                &mut right_voice,
                &mut left_failed_selection,
                &mut right_failed_selection,
                &mut scratch,
                &mut buf_a,
            ),
        )
        .await;
        if i2s_underran() {
            underruns += 1;
            clear_i2s_underrun();
        }

        // Report only when the count moved, and at most every 2s, so a clean run
        // is silent and a bad run doesn't flood the log (which would make it worse).
        let now = Instant::now();
        if underruns != reported && (now - last_report).as_millis() >= 2000 {
            println!(
                "AUDIO underruns: {} total (+{} in {}ms)",
                underruns,
                underruns - reported,
                (now - last_report).as_millis(),
            );
            reported = underruns;
            last_report = now;
        }
    }
}
