use defmt::println;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embedded_sdmmc::Mode;

use crate::periphs::sd::{self, SdFile, SdHandle};

// Raw bytes moved per SD read. Matches the SD card's native block size.
pub const SD_CHUNK_SIZE: usize = 512;
// 8 chunks -> 4 KiB buffered ahead per voice, matching the read-ahead buffer
// size the single-core version used.
const CHUNK_QUEUE_DEPTH: usize = 8;

/// What core0 wants core1 to be streaming for one voice. `generation` is bumped by
/// core0 on every new selection so core1's events - and anything already
/// in flight when the selection changes - can be told apart from a
/// since-replaced selection. There is no explicit "flush the channel"
/// operation; stale events are just discarded by core0 on a `generation` mismatch.
#[derive(Clone)]
pub struct StreamSelection {
    pub filename: heapless::String<{ crate::config::MAX_FILENAME_LEN }>,
    pub loop_playback: bool,
    pub generation: u32,
}

/// One update from core1's SD reader back to core0's decoder.
pub enum StreamEvent {
    Data {
        generation: u32,
        len: u16,
        buf: [u8; SD_CHUNK_SIZE],
    },
    /// Non-looping playback ran off the end of the file.
    Eof {
        generation: u32,
    },
    /// The file named in the selection could not be opened.
    OpenFailed {
        generation: u32,
    },
}

/// Cross-core link for one voice (left or right). Core0 posts the latest
/// desired selection; core1 owns the SD card and streams raw file bytes back.
/// Both halves are safe to share across cores: `Signal`/`Channel` use
/// `CriticalSectionRawMutex`, which is cross-core-safe with the
/// `critical-section-impl` feature enabled on `embassy-rp`.
pub struct VoiceChannel {
    pub selection: Signal<CriticalSectionRawMutex, Option<StreamSelection>>,
    pub events: Channel<CriticalSectionRawMutex, StreamEvent, CHUNK_QUEUE_DEPTH>,
}

impl VoiceChannel {
    pub const fn new() -> Self {
        Self {
            selection: Signal::new(),
            events: Channel::new(),
        }
    }
}

pub static LEFT_CHANNEL: VoiceChannel = VoiceChannel::new();
pub static RIGHT_CHANNEL: VoiceChannel = VoiceChannel::new();

/// Reads the first 10 bytes of an MP3 and, if they are an ID3v2 tag header,
/// returns the byte offset where the actual audio starts. Real songs routinely
/// carry tens of KB of ID3v2 metadata (embedded album art); seeking past it up
/// front keeps the decoder from grinding through all of it on every file open.
/// Returns 0 when there is no recognisable tag.
fn id3v2_data_start(file: &mut SdFile<'static>) -> u32 {
    let mut header = [0u8; 10];
    let mut read = 0;

    while read < header.len() {
        match file.read(&mut header[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => break,
        }
    }

    // "ID3" magic, and the 4 size bytes must be syncsafe (top bit clear).
    let is_id3 = read == header.len()
        && &header[..3] == b"ID3"
        && header[6] < 0x80
        && header[7] < 0x80
        && header[8] < 0x80
        && header[9] < 0x80;

    if !is_id3 {
        return 0;
    }

    let size = ((header[6] as u32) << 21)
        | ((header[7] as u32) << 14)
        | ((header[8] as u32) << 7)
        | (header[9] as u32);

    let mut total = 10 + size;
    if header[5] & 0x10 != 0 {
        total += 10; // optional footer
    }

    total
}

fn open_selection(handle: SdHandle, sel: &StreamSelection) -> Option<(SdFile<'static>, u32)> {
    match sd::open_file(handle, sel.filename.as_str(), Mode::ReadOnly) {
        Ok(mut file) => {
            let data_start = id3v2_data_start(&mut file);
            if file.seek_from_start(data_start).is_err() {
                let _ = file.seek_from_start(0);
            }
            println!(
                "Audio SD: streaming {} (loop={}, generation={}, data_start={})",
                sel.filename.as_str(),
                sel.loop_playback,
                sel.generation,
                data_start
            );
            Some((file, data_start))
        }
        Err(error) => {
            println!(
                "Audio SD: failed to open {}: {:?}",
                sel.filename.as_str(),
                defmt::Debug2Format(&error)
            );
            None
        }
    }
}

/// Runs on core1. Owns the SD card for one voice (left or right): waits for a
/// selection from core0, opens the file, and streams it back as fixed-size
/// chunks. Handles `Loop` mode's rewind-on-EOF itself, so core0 never touches
/// the SD card at all. Spawned twice (`pool_size = 2`), once per side.
#[embassy_executor::task(pool_size = 2)]
pub async fn sd_voice_task(handle: SdHandle, channel: &'static VoiceChannel) {
    // (file, loop_playback, data_start, generation)
    let mut current: Option<(SdFile<'static>, bool, u32, u32)> = None;
    let mut chunks_sent: u32 = 0;

    loop {
        let update = if current.is_some() {
            channel.selection.try_take()
        } else {
            Some(channel.selection.wait().await)
        };

        if let Some(selection) = update {
            current = None;
            chunks_sent = 0;
            if let Some(sel) = selection {
                if let Some((file, data_start)) = open_selection(handle, &sel) {
                    current = Some((file, sel.loop_playback, data_start, sel.generation));
                } else {
                    channel
                        .events
                        .send(StreamEvent::OpenFailed { generation: sel.generation })
                        .await;
                }
            }
            continue;
        }

        let Some((file, loop_playback, data_start, generation)) = current.as_mut() else {
            continue;
        };
        let generation = *generation;

        let mut buf = [0u8; SD_CHUNK_SIZE];
        match file.read(&mut buf) {
            Ok(0) => {
                let rewound = *loop_playback && file.seek_from_start(*data_start).is_ok();
                if rewound {
                    continue;
                }
                println!(
                    "Audio SD: generation={} hit EOF after {} chunks",
                    generation, chunks_sent
                );
                channel.events.send(StreamEvent::Eof { generation }).await;
                current = None;
            }
            Ok(n) => {
                chunks_sent += 1;
                if chunks_sent == 1 || chunks_sent % 100 == 0 {
                    println!(
                        "Audio SD: generation={} sent chunk #{} ({} bytes)",
                        generation, chunks_sent, n
                    );
                }
                channel
                    .events
                    .send(StreamEvent::Data {
                        generation,
                        len: n as u16,
                        buf,
                    })
                    .await;
            }
            Err(error) => {
                println!(
                    "Audio SD: generation={} read error after {} chunks: {:?}",
                    generation,
                    chunks_sent,
                    defmt::Debug2Format(&error)
                );
                channel.events.send(StreamEvent::Eof { generation }).await;
                current = None;
            }
        }
    }
}
