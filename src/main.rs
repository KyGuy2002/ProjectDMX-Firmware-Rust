#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

mod config;
mod hardware;
mod modules;
mod pixel_mapping_config;

mod periphs {
    pub mod dmx;
    pub mod eth;
    pub mod artnet;
    pub mod sacn;
    pub mod oled;
    pub mod sensors;
    pub mod tcp_cmds;
    pub mod sd;
    pub mod sd_stream;
    pub mod audio;
}

use core::cell::RefCell;
use core::future::pending;
use core::ptr::addr_of_mut;

use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_rp::interrupt;
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_sync::once_lock::OnceLock;
use embassy_net::Ipv4Address;
use static_cell::StaticCell;

use config::*;
use modules::*;


use crate::hardware::AssignedResources;
use crate::hardware::*;




// Global config
pub static CONFIG: OnceLock<BoardInstanceConfig> = OnceLock::new();

// Global DMX buffer
pub const MAX_UNIVERSES: usize = 4;
// Per-port NeoPixel buffer size. Must cover the longest strip in any config
// (currently the 240-pixel glowing-wire runs). Sizes the neo buffers, the
// layout table, and the PioWs2812 const generic.
pub const MAX_PIXELS: usize = 256;

pub static DMX_MATRIX: BlockingMutex<CriticalSectionRawMutex, RefCell<[[u8; 512]; MAX_UNIVERSES]>> =
    BlockingMutex::new(RefCell::new([[0u8; 512]; MAX_UNIVERSES]));

static IP_STATE: StaticCell<AsyncMutex<CriticalSectionRawMutex, Option<Ipv4Address>>> = StaticCell::new();

// The audio task runs on its own high-priority interrupt executor so it preempts
// every thread-mode task (OLED I2C flush, NeoPixel effects, sACN parsing, ...).
// The I2S PIO FIFO only holds ~180us of samples between DMA transfers; any
// cooperative task that blocks the shared executor longer than that at a buffer
// boundary causes an audible underrun. Preemption removes that whole class of
// glitch. P3 keeps it below the hardware driver IRQs (DMA/timer) it depends on.
static AUDIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn SWI_IRQ_1() {
    unsafe { AUDIO_EXECUTOR.on_interrupt() };
}

// Core1 does nothing but own the SD card: it waits for a (filename, loop?)
// selection from each audio voice and streams raw file bytes back over
// `periphs::sd_stream`'s channels. The blocking SPI reads that used to run
// inside the P3 audio ISR on core0 (stalling DMX/dimmer/OLED/neo for their
// duration) now happen entirely on core1, where they can't preempt anything.
static mut CORE1_STACK: Stack<16384> = Stack::new();
static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();


#[embassy_executor::main]
async fn main(spawner: Spawner) {


    info!("=======================================");
    info!("");
    info!("     ProjectDMX Controller Booting     ");
    info!("              Version 0.1r             ");
    info!("");



    // Embassy init
    let hardware_config = embassy_rp::config::Config::default();
    let p = embassy_rp::init(hardware_config);



    // Manage pins and peripherals
    let r = split_resources!(p);



    // JSONC Configuration. `None` = use the compiled-in config.jsonc; pass
    // Some(sd_text) here once SD-card config loading lands.
    let config = load_config(None);
    CONFIG.init(config.clone()).unwrap();



    // Spawn Peripherals
    let ip_state = IP_STATE.init(AsyncMutex::new(None));
    spawner.spawn(periphs::oled::oled_task(r.oled, ip_state)).unwrap(); // OLED
    spawner.spawn(periphs::dmx::dmx_task(r.dmx)).unwrap(); // DMX

    // SD card + MP3 file reading live entirely on core1: those SPI reads used
    // to run inline inside the P3 audio ISR on core0, blocking DMX/dimmer/OLED/
    // neo for however long each read took. Core1 mounts the card itself (its
    // VolumeManager holds a RefCell - not Send - so the handle must never
    // leave the executor it's used on) and streams raw file bytes back to
    // core0's decoder over `periphs::sd_stream`'s per-voice channels.
    spawn_core1(
        p.CORE1,
        unsafe { &mut *addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = CORE1_EXECUTOR.init(Executor::new());
            executor1.run(|spawner| {
                let handle = periphs::sd::init(r.sd);
                spawner
                    .spawn(periphs::sd_stream::sd_voice_task(
                        handle,
                        &periphs::sd_stream::LEFT_CHANNEL,
                    ))
                    .unwrap();
                spawner
                    .spawn(periphs::sd_stream::sd_voice_task(
                        handle,
                        &periphs::sd_stream::RIGHT_CHANNEL,
                    ))
                    .unwrap();
            });
        },
    );

    // DMX-triggered audio playback - on a dedicated high-priority interrupt
    // executor so decode/DMA servicing preempts the thread-mode tasks. It only
    // ever sees raw bytes handed to it from core1 - never the SD card itself.
    interrupt::SWI_IRQ_1.set_priority(Priority::P3);
    let audio_spawner = AUDIO_EXECUTOR.start(interrupt::SWI_IRQ_1);
    audio_spawner
        .spawn(periphs::audio::audio_task(
            config.audio,
            r.audio,
            &periphs::sd_stream::LEFT_CHANNEL,
            &periphs::sd_stream::RIGHT_CHANNEL,
        ))
        .unwrap();

    if config.input.source == InputProtocol::Artnet || config.input.source == InputProtocol::sACN {
        let stack = periphs::eth::start_eth(&spawner, r.eth, ip_state).await; // Ethernet
        periphs::sensors::start_sensors(&spawner, r.sensors); // Sensors
        spawner.spawn(periphs::tcp_cmds::tcp_cmds_task(stack)).unwrap(); // TCP Commands

        if config.input.source == InputProtocol::Artnet {
            spawner.spawn(periphs::artnet::artnet_task(stack)).unwrap(); // Art-Net
        } else if config.input.source == InputProtocol::sACN {
            spawner.spawn(periphs::sacn::sacn_task(stack)).unwrap(); // sACN
        }
    }



    // Module Initialization
    // init_slot_a(&spawner, config.modules.slot_a, r.slot_a_relay);
    init_slot_b(&spawner, config.modules.slot_b, r.slot_b_dimmer);
    init_slot_c(&spawner, config.modules.slot_c, r.slot_c_neo);
    init_slot_d(&spawner, config.modules.slot_d, r.slot_d_dimmer);



    pending::<()>().await;


}








/**
 * Reads a slice of DMX channel values from the DMX_MATRIX for a given universe and starting channel.
 *
 * Both `universe` (1..=MAX_UNIVERSES) and `start_channel` (1..=512) are 1-based,
 * matching how fixtures and the JSON config are addressed. `DMX_MATRIX` is stored
 * 0-based, so universe 1 / channel 1 is row 0 / index 0. Out-of-range requests,
 * and any tail past channel 512, read as 0.
 */
pub fn read_channels<const N: usize>(universe: usize, start_channel: usize) -> [u8; N] {
    DMX_MATRIX.lock(|matrix| {
        let mut dest = [0u8; N];

        if (1..=MAX_UNIVERSES).contains(&universe) && (1..=512).contains(&start_channel) {
            let universe_index = universe - 1;
            let start_index = start_channel - 1;
            let buf = matrix.borrow();
            let universe_row: &[u8] = &buf[universe_index];

            let end = (start_index + N).min(512);
            let src_slice = &universe_row[start_index..end];

            dest[..src_slice.len()].copy_from_slice(src_slice);
        }

        dest
    })
}