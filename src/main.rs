#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::Timer;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);

    let mut count = 0u32;

    loop {
        info!("blink {}", count);

        led.set_high();
        Timer::after_millis(500).await;

        led.set_low();
        Timer::after_millis(500).await;

        count = count.wrapping_add(1);
    }
}