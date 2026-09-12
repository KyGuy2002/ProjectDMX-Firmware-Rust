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
    pub mod audio;
}

use core::cell::RefCell;
use core::future::pending;

use embassy_executor::{Executor, Spawner};
use embassy_net::Ipv4Address;
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_sync::once_lock::OnceLock;
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

// Audio gets the RP2350's second core to itself. The task future (including its
// large decode and double buffers) is allocated statically by Embassy; this
// stack only needs to cover the synchronous polling/decode call chain.
static mut CORE1_STACK: Stack<32768> = Stack::new();
static AUDIO_EXECUTOR: StaticCell<Executor> = StaticCell::new();

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

    // DMX-triggered audio playback runs on core 1. The SD card is mounted inside
    // the task: its VolumeManager holds a RefCell (not Send), so the handle is
    // created and remains entirely on the audio core.
    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor = AUDIO_EXECUTOR.init(Executor::new());
            executor.run(|audio_spawner| {
                audio_spawner
                    .spawn(periphs::audio::audio_task(config.audio, r.audio, r.sd))
                    .unwrap()
            })
        },
    );

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
