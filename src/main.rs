#![no_std]
#![no_main]

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};

use ssd1306::{
    prelude::*,
    I2CDisplayInterface,
    Ssd1306
};

use embassy_rp::i2c::{Config, I2c};

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_time::Timer;

use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program, Grb};
use smart_leds::RGB8;
use static_cell::StaticCell;


bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const NUM_LEDS: usize = 30;



#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut i2c_cfg = Config::default();
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


    // WS2812 / NeoPixel setup on GPIO 16
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



    let mut count = 0u32;



    loop {
        info!("blink {}", count);

        leds.fill(RGB8 { r: 255, g: 0, b: 0 });
        ws2812.write(&leds).await;
        Timer::after_millis(500).await;

        leds.fill(RGB8 { r: 0, g: 255, b: 0 });
        ws2812.write(&leds).await;
        Timer::after_millis(500).await;

        leds.fill(RGB8 { r: 0, g: 0, b: 255 });
        ws2812.write(&leds).await;
        Timer::after_millis(500).await;

        leds.fill(RGB8 { r: 0, g: 0, b: 0 });
        ws2812.write(&leds).await;
        Timer::after_millis(500).await;

        count = count.wrapping_add(1);
    }
}