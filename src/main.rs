#![no_std]
#![no_main]

mod config;
mod dmx_state;
mod neo_effects;

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};

use embassy_rp::gpio::{Level, Output};
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, UART1};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{Grb, PioWs2812, PioWs2812Program};
use smart_leds::RGB8;
use static_cell::StaticCell;

use embassy_rp::uart::{
    Async, Config as UartConfig, DataBits, Parity, StopBits, Uart, UartRx,
};

use config::{get_layout_map, PixelMeta, NUM_LEDS};
use dmx_state::{DmxParams, DMX_SIGNAL};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    UART1_IRQ => embassy_rp::uart::InterruptHandler<UART1>;
});

/// Tracks running crossfader configurations without dynamic heap allocations.
enum TransitionState {
    Stable,
    Crossfading {
        old_params: DmxParams,
        progress: u8,
        duration: u8,
    },
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // RS-485 transceiver enable, active-low.
    let _rs485_enable = Output::new(p.PIN_23, Level::Low);

    // DMX UART Peripheral initialization: 250000 baud, 8N2.
    let mut dmx_uart_cfg = UartConfig::default();
    dmx_uart_cfg.baudrate = 250_000;
    dmx_uart_cfg.data_bits = DataBits::DataBits8;
    dmx_uart_cfg.stop_bits = StopBits::STOP2;
    dmx_uart_cfg.parity = Parity::ParityNone;

    let dmx_uart = Uart::new(
        p.UART1,
        p.PIN_24, // Unused TX Pin allocation
        p.PIN_25, // RX Pin input
        Irqs,
        p.DMA_CH1,
        p.DMA_CH2,
        dmx_uart_cfg,
    );

    let (_dmx_tx, dmx_rx) = dmx_uart.split();
    spawner.spawn(dmx_rx_task(dmx_rx)).unwrap();

    // WS2812 PIO Motor Driver instantiation on GPIO9
    let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);

    static PROGRAM: StaticCell<PioWs2812Program<PIO0>> = StaticCell::new();
    let program = PROGRAM.init(PioWs2812Program::new(&mut common));

    let mut ws2812 = PioWs2812::<PIO0, 0, NUM_LEDS, Grb>::new(
        &mut common,
        sm0,
        p.DMA_CH0,
        p.PIN_9,
        program,
    );

    // Global pre-calculated coordinate space mapping allocation
    let layout_table: [PixelMeta; NUM_LEDS] = get_layout_map();

    let mut leds_output = [RGB8::default(); NUM_LEDS];

    let mut active_params = DmxParams::default();
    let mut transition = TransitionState::Stable;

    let mut base_offset: u8 = 0;
    let mut top_offset: u8 = 0;

    let mut ticker = Ticker::every(Duration::from_millis(20)); // Clean ~50FPS Refresh rate

    loop {
        // Look for a non-blocking incoming DMX update
        if let Some(new_dmx) = DMX_SIGNAL.try_take() {
            if new_dmx.base_effect_id != active_params.base_effect_id 
                || new_dmx.top_effect_id != active_params.top_effect_id 
            {
                // Trigger a smooth crossfade over 25 frame updates
                transition = TransitionState::Crossfading {
                    old_params: active_params,
                    progress: 0,
                    duration: 25,
                };
            }
            active_params = new_dmx;
        }

        // Apply speed increments based on parsed values
        base_offset = base_offset.wrapping_add(active_params.speed.clamp(1, 15));
        top_offset = top_offset.wrapping_add(active_params.speed.clamp(1, 15));

        match transition {
            TransitionState::Stable => {
                // Single-pass inline processing
                for i in 0..NUM_LEDS {
                    let meta = &layout_table[i];
                    let base_color = neo_effects::render_base_effect(active_params.base_effect_id, base_offset, &active_params, meta);
                    leds_output[i] = neo_effects::apply_top_effect(active_params.top_effect_id, top_offset, base_color, meta);
                }
            }
            TransitionState::Crossfading { old_params, ref mut progress, duration } => {
                *progress += 1;
                let alpha = ((*progress as u16) * 256) / (duration as u16);

                for i in 0..NUM_LEDS {
                    let meta = &layout_table[i];

                    // Process history/source track frame values
                    let old_base = neo_effects::render_base_effect(old_params.base_effect_id, base_offset, &old_params, meta);
                    let old_composite = neo_effects::apply_top_effect(old_params.top_effect_id, top_offset, old_base, meta);

                    // Process target frame destination parameters
                    let new_base = neo_effects::render_base_effect(active_params.base_effect_id, base_offset, &active_params, meta);
                    let new_composite = neo_effects::apply_top_effect(active_params.top_effect_id, top_offset, new_base, meta);

                    // Mix frames into hardware output cleanly
                    leds_output[i] = neo_effects::blend_rgb(old_composite, new_composite, alpha);
                }

                if *progress >= duration {
                    transition = TransitionState::Stable;
                }
            }
        }

        // Stream frame via DMA to PIO
        ws2812.write(&leds_output).await;
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn dmx_rx_task(mut rx: UartRx<'static, Async>) {
    let mut frame = [0u8; 513];
    loop {
        match rx.read(&mut frame).await {
            Ok(_) => {

                const START_CH: usize = 7;

                // Map your console channels directly onto DMX structural fields
                let extracted = DmxParams {
                    r: frame[START_CH + 0],
                    g: frame[START_CH + 1],
                    b: frame[START_CH + 2],
                    base_effect_id: frame[START_CH + 3],
                    top_effect_id: frame[START_CH + 4],
                    speed: frame[START_CH + 5],
                };
                DMX_SIGNAL.signal(extracted);
            }
            Err(embassy_rp::uart::Error::Break) => {}
            Err(embassy_rp::uart::Error::Framing) => {}
            Err(e) => {
                warn!("DMX connection drop or frame issue: {:?}", e);
            }
        }
    }
}