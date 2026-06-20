//! Hello World Example for pico_rust_template
//!
//! This example demonstrates basic setup:
//! - Initializes the RP2354B
//! - Blinks the onboard LED (if present) or a GPIO
//! - Prints debug messages via defmt/RTT

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::Timer;

#[cfg(feature = "defmt")]
use defmt::*;

#[cfg(feature = "defmt")]
use defmt_rtt as _;

#[cfg(feature = "defmt")]
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    #[cfg(feature = "defmt")]
    info!("Hello from pico_rust_template!");

    // Initialize the RP2354B peripherals
    let p = embassy_rp::init(Default::default());

    // Use GP25 as LED pin (standard Pico LED location)
    // Note: Your board may have LED on a different pin
    let mut led = Output::new(p.PIN_25, Level::Low);

    #[cfg(feature = "defmt")]
    info!("Starting LED blink loop...");

    let mut counter: u32 = 0;

    loop {
        // Turn LED on
        led.set_high();

        #[cfg(feature = "defmt")]
        info!("LED ON  - count: {}", counter);

        Timer::after_millis(500).await;

        // Turn LED off
        led.set_low();

        #[cfg(feature = "defmt")]
        info!("LED OFF - count: {}", counter);

        Timer::after_millis(500).await;

        counter = counter.wrapping_add(1);
    }
}
