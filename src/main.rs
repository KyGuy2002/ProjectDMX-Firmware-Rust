#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_time::Timer;

// OLED
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use embassy_rp::i2c::{Config as I2cConfig, I2c};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

// GPIO
use embassy_rp::gpio::{Level, Output};

// NeoPixel
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, UART1};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{Grb, PioWs2812, PioWs2812Program};
use smart_leds::RGB8;
use static_cell::StaticCell;

// DMX / UART
use embassy_rp::uart::{
    Async,
    Config as UartConfig,
    DataBits,
    Parity,
    StopBits,
    Uart,
    UartRx,
};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    UART1_IRQ => embassy_rp::uart::InterruptHandler<UART1>;
});

const NUM_LEDS: usize = 30;


use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    signal::Signal,
};

static DMX_DIMMER: Signal<CriticalSectionRawMutex, u8> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // RS-485 transceiver enable, active-low.
    let _rs485_enable = Output::new(p.PIN_23, Level::Low);

    // DMX UART: 250000 baud, 8N2.
    // UART1 TX = GPIO24, UART1 RX = GPIO25.
    let mut dmx_uart_cfg = UartConfig::default();
    dmx_uart_cfg.baudrate = 250_000;
    dmx_uart_cfg.data_bits = DataBits::DataBits8;
    dmx_uart_cfg.stop_bits = StopBits::STOP2;
    dmx_uart_cfg.parity = Parity::ParityNone;

    let dmx_uart = Uart::new(
        p.UART1,
        p.PIN_24,      // UART1 TX, unused
        p.PIN_25,      // UART1 RX from RS-485 RO
        Irqs,
        p.DMA_CH1,
        p.DMA_CH2,
        dmx_uart_cfg,
    );

    let (_dmx_tx, dmx_rx) = dmx_uart.split();
    spawner.spawn(dmx_rx_task(dmx_rx)).unwrap();

    // OLED setup
    let mut i2c_cfg = I2cConfig::default();
    i2c_cfg.frequency = 400_000;

    let i2c = I2c::new_blocking(
        p.I2C1,
        p.PIN_27,
        p.PIN_38,
        i2c_cfg,
    );

    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306::new(
        interface,
        DisplaySize128x64,
        DisplayRotation::Rotate0,
    )
    .into_buffered_graphics_mode();

    display.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline(
        "Hello RP2040!",
        Point::new(0, 0),
        text_style,
        Baseline::Top,
    )
    .draw(&mut display)
    .unwrap();

    display.flush().unwrap();

    // WS2812 / NeoPixel setup on GPIO9
    let Pio {
        mut common,
        sm0,
        ..
    } = Pio::new(p.PIO0, Irqs);

    static PROGRAM: StaticCell<PioWs2812Program<PIO0>> = StaticCell::new();
    let program = PROGRAM.init(PioWs2812Program::new(&mut common));

    let mut ws2812 = PioWs2812::<PIO0, 0, NUM_LEDS, Grb>::new(
        &mut common,
        sm0,
        p.DMA_CH0,
        p.PIN_9,
        program,
    );

    let mut leds = [RGB8::default(); NUM_LEDS];

    loop {
        let dimmer = DMX_DIMMER.wait().await;

        leds.fill(RGB8 {
            r: dimmer,
            g: dimmer,
            b: dimmer,
        });

        ws2812.write(&leds).await;
    }
}

#[embassy_executor::task]
async fn dmx_rx_task(mut rx: UartRx<'static, Async>) {
    let mut frame = [0u8; 513];
    let mut frame_count = 0u32;

    loop {
        match rx.read(&mut frame).await {
            Ok(_) => {


                info!(
                    "RAW: {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    frame[0], frame[1], frame[2], frame[3],
                    frame[4], frame[5], frame[6], frame[7],
                    frame[8], frame[9], frame[10], frame[11],
                    frame[12], frame[13], frame[14], frame[15],
                );


                let ch6 = frame[10];
                DMX_DIMMER.signal(ch6);

                frame_count = frame_count.wrapping_add(1);

                if frame_count % 20 == 0 {
                    info!("DMX ch10 dimmer: {}", ch6);
                }
            }
            Err(embassy_rp::uart::Error::Break) => {}
            Err(embassy_rp::uart::Error::Framing) => {}
            Err(e) => {
                warn!("DMX read error: {:?}", e);
            }
        }
    }
}