use embassy_time::{Duration, Timer};
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use defmt::info;

use crate::config::DimmerConfig;
use crate::hardware::{SlotBDimmerResources, SlotDDimmerResources};
use crate::read_channels;

/// Reads the port's 4 DMX bytes and applies each output's binary flag:
/// `true` -> full-on above DMX 127 / off below, `false` -> linear passthrough.
fn resolve_levels(settings: &DimmerConfig) -> [u16; 4] {
    let ch = read_channels::<4>(settings.universe as usize, settings.start_channel as usize);
    core::array::from_fn(|i| {
        if settings.binary[i] {
            if ch[i] > 127 { 255 } else { 0 }
        } else {
            ch[i] as u16
        }
    })
}

/// Slot D: 4 outputs on 2 paired slices. Pin order fixed by the PCB (hardware.rs):
///   out0 = pin1 = SLICE1 chan B     out1 = pin2 = SLICE1 chan A
///   out2 = pin3 = SLICE0 chan B     out3 = pin4 = SLICE0 chan A
#[embassy_executor::task]
pub async fn dimmer_slot_d_task(settings: DimmerConfig, r: SlotDDimmerResources) {
    info!("Starting dimmer task (slot D)");

    let mut cfg_slice1 = PwmConfig::default();
    cfg_slice1.top = 255;
    let mut cfg_slice0 = PwmConfig::default();
    cfg_slice0.top = 255;

    let mut pwm_slice1 = Pwm::new_output_ab(r.pwm1, r.pin2, r.pin1, cfg_slice1.clone());
    let mut pwm_slice0 = Pwm::new_output_ab(r.pwm0, r.pin4, r.pin3, cfg_slice0.clone());

    loop {
        let lv = resolve_levels(&settings);

        cfg_slice1.compare_b = lv[0]; // out0 / pin1
        cfg_slice1.compare_a = lv[1]; // out1 / pin2
        cfg_slice0.compare_b = lv[2]; // out2 / pin3
        cfg_slice0.compare_a = lv[3]; // out3 / pin4

        pwm_slice1.set_config(&cfg_slice1);
        pwm_slice0.set_config(&cfg_slice0);

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Slot B: each output sits on its own slice (single channel), because slot B's
/// four pins land on four different slices. Pin -> slice/chan fixed by the PCB
/// (hardware.rs):
///   out0 = pin1 = SLICE4 chan A     out1 = pin2 = SLICE3 chan A
///   out2 = pin3 = SLICE2 chan B     out3 = pin4 = SLICE8 chan B
#[embassy_executor::task]
pub async fn dimmer_slot_b_task(settings: DimmerConfig, r: SlotBDimmerResources) {
    info!("Starting dimmer task (slot B)");

    let mut c0 = PwmConfig::default();
    c0.top = 255;
    let mut c1 = PwmConfig::default();
    c1.top = 255;
    let mut c2 = PwmConfig::default();
    c2.top = 255;
    let mut c3 = PwmConfig::default();
    c3.top = 255;

    let mut out0 = Pwm::new_output_a(r.pwm1, r.pin1, c0.clone());
    let mut out1 = Pwm::new_output_a(r.pwm2, r.pin2, c1.clone());
    let mut out2 = Pwm::new_output_b(r.pwm3, r.pin3, c2.clone());
    let mut out3 = Pwm::new_output_b(r.pwm4, r.pin4, c3.clone());

    loop {
        let lv = resolve_levels(&settings);

        c0.compare_a = lv[0];
        c1.compare_a = lv[1];
        c2.compare_b = lv[2];
        c3.compare_b = lv[3];

        out0.set_config(&c0);
        out1.set_config(&c1);
        out2.set_config(&c2);
        out3.set_config(&c3);

        Timer::after(Duration::from_millis(1)).await;
    }
}
