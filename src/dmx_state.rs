//! Safe structured extraction of incoming DMX commands.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DmxParams {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub base_effect_id: u8,
    pub top_effect_id: u8,
    pub speed: u8,
}

impl Default for DmxParams {
    fn default() -> Self {
        Self {
            r: 255,
            g: 255,
            b: 255,
            base_effect_id: 1, // Default base: 4th July Wheel
            top_effect_id: 0,  // Default top: None
            speed: 5,
        }
    }
}

// Thread-safe mechanism passing atomic updates between frames
pub static DMX_SIGNAL: Signal<CriticalSectionRawMutex, DmxParams> = Signal::new();