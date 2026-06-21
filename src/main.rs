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



    let mut count = 0u32;



    loop {
        info!("blink {}", count);

        Timer::after_millis(500).await;

        Timer::after_millis(500).await;

        count = count.wrapping_add(1);
    }
}