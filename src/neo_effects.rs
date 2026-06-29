// src/neo_effects.rs

use smart_leds::RGB8;
use crate::config::{PixelMeta};
use crate::dmx_state::DmxParams;

/// Converts logical colors directly to physical layout mapping rules (GRB Swapped)
pub fn rgb_fixed(r: u8, g: u8, b: u8) -> RGB8 {
    RGB8::new(g, r, b)
}

/// Fixed point interpolation between frames
pub fn blend_rgb(from: RGB8, to: RGB8, alpha: u16) -> RGB8 {
    let inv_alpha = 256u16.saturating_sub(alpha);
    RGB8 {
        r: ((from.r as u16 * inv_alpha + to.r as u16 * alpha) >> 8) as u8,
        g: ((from.g as u16 * inv_alpha + to.g as u16 * alpha) >> 8) as u8,
        b: ((from.b as u16 * inv_alpha + to.b as u16 * alpha) >> 8) as u8,
    }
}

/// Generates base background imagery onto buffers
pub fn render_base_effect(
    id: u8,
    offset: u8,
    params: &DmxParams,
    meta: &PixelMeta,
) -> RGB8 {
    if !meta.is_valid {
        return RGB8::default();
    }

    match id {
        // 1: Original 4th of July continuous layout effect (re-mapped smoothly using physical coordinates)
        1 => {
            // Instead of using an external index lookup, we use your original formula's logic
            // mapped continuously across the individual letter's relative coordinate space.
            let pos = meta.y.wrapping_add(offset);
            patriotic_wheel(pos)
        }

        // 2: Continuous multi-letter Pinwheel layout mapping
        2 => {
            let pos = meta.x.wrapping_add(offset);
            wheel(pos)
        }

        // 3: DMX Driven Custom Color Stardust Twinkle
        3 => {
            let pseudo_rand = (meta.index as u8).wrapping_mul(37).wrapping_add(offset);
            if pseudo_rand > 245 {
                rgb_fixed(params.r, params.g, params.b)
            } else {
                RGB8::default()
            }
        }

        // 4: Solid static uniform color mapping
        4 => rgb_fixed(params.r, params.g, params.b),

        // 5: 4th July Static letter assignments (U=Red, S=White, A=Blue, Mirroring to 250)
        5 => match meta.letter_id {
            0 | 5 => rgb_fixed(255, 0, 0),     // U and 0 -> Red
            1 | 4 => rgb_fixed(255, 255, 255), // S and 5 -> White
            2 | 3 => rgb_fixed(0, 0, 255),     // A and 2 -> Blue
            _ => RGB8::default(),
        }

        // 255: Diagnostic Mode - Light a single pixel by raw index using the speed channel value
        255 => {
            // Use the speed parameter directly as the target pixel index
            if meta.index == (params.speed as usize) {
                rgb_fixed(255, 255, 255) // Solid white for the identified pixel
            } else {
                RGB8::default() // Turn everything else off
            }
        }

        _ => RGB8::default(),
    }
}

pub fn apply_top_effect(
    id: u8,
    frame_counter: u8, 
    bg_color: RGB8,
    meta: &PixelMeta,
    params: &DmxParams, // <-- Added this to get access to the speed channel values
) -> RGB8 {
    if !meta.is_valid {
        return RGB8::default();
    }

    // -------------------------------------------------------------
    // SWEEP DIMENSION CONFIGURATION
    // -------------------------------------------------------------
    let beam_width: i32 = 25; 
    let fade_range: i32 = 20; 
    let max_window = beam_width + fade_range;

    let dimmed_bg = RGB8 {
        r: bg_color.r >> 2,
        g: bg_color.g >> 2,
        b: bg_color.b >> 2,
    };

    let target_pos = (frame_counter as i32 * 340) / 255 - 45;

    match id {
        // 1..4 Horizontal panning sweeps
        1 | 2 | 3 | 4 => {
            let x_val = meta.x as i32;

            let dist = match id {
                1 => (x_val - target_pos).abs(),
                2 => ((255 - x_val) - target_pos).abs(),
                3 => ((128 - x_val).abs() - target_pos).abs(),
                4 => ((x_val - 128).abs() - target_pos).abs(),
                _ => 255,
            };

            if dist <= beam_width {
                rgb_fixed(255, 255, 255)
            } else if dist <= max_window {
                let alpha = ((max_window - dist) * 256) / fade_range;
                blend_rgb(dimmed_bg, rgb_fixed(255, 255, 255), alpha as u16)
            } else {
                dimmed_bg
            }
        }

        // 5..6 Vertical panning sweeps
        5 | 6 => {
            let y_val = meta.y as i32;

            let dist = match id {
                5 => (y_val - target_pos).abs(),       
                6 => ((255 - y_val) - target_pos).abs(), 
                _ => 255,
            };

            if dist <= beam_width {
                rgb_fixed(255, 255, 255) 
            } else if dist <= max_window {
                let alpha = ((max_window - dist) * 256) / fade_range;
                blend_rgb(dimmed_bg, rgb_fixed(255, 255, 255), alpha as u16)
            } else {
                dimmed_bg
            }
        }

        // 7: Segmented Letter Flash Strobe (Slower baseline with Fade option)
        7 => {
            let fade_threshold = 50u8;
            let slow_tick = frame_counter >> 3; 
            let pseudo_rand = (meta.letter_id as u8).wrapping_mul(79).wrapping_add(slow_tick);
            let is_on = (pseudo_rand % 3) == 0;

            if is_on {
                bg_color
            } else if params.speed < fade_threshold {
                let alpha = (params.speed as i32 * 256) / fade_threshold as i32;
                blend_rgb(RGB8::default(), bg_color, alpha as u16)
            } else {
                RGB8::default() 
            }
        }

        // 8: Out-to-In Letter Sweep (U & 0 -> S & 5 -> A & 2)
        // 9: In-to-Out Letter Sweep (A & 2 -> S & 5 -> U & 0)
        8 | 9 => {
            let fade_threshold = 50u8;
            let step = frame_counter / 86; 

            let is_targeted = match id {
                8 => match step {
                    0 => meta.letter_id == 0 || meta.letter_id == 5,
                    1 => meta.letter_id == 1 || meta.letter_id == 4,
                    _ => meta.letter_id == 2 || meta.letter_id == 3,
                },
                9 => match step {
                    0 => meta.letter_id == 2 || meta.letter_id == 3,
                    1 => meta.letter_id == 1 || meta.letter_id == 4,
                    _ => meta.letter_id == 0 || meta.letter_id == 5,
                },
                _ => false,
            };

            let dimmed_bg = RGB8 {
                r: ((bg_color.r as u16 * 25) >> 8) as u8,
                g: ((bg_color.g as u16 * 25) >> 8) as u8,
                b: ((bg_color.b as u16 * 25) >> 8) as u8,
            };

            if is_targeted {
                bg_color 
            } else if params.speed < fade_threshold {
                let alpha = (params.speed as i32 * 256) / fade_threshold as i32;
                blend_rgb(dimmed_bg, bg_color, alpha as u16)
            } else {
                dimmed_bg 
            }
        }

        _ => bg_color,
    }
}

fn patriotic_wheel(pos: u8) -> RGB8 {
    let section = pos / 64;
    let step = (pos % 64) as u16;
    match section {
        0 => { let v = fade(step); rgb_fixed(255, v, v) }
        1 => { let v = fade(63 - step); rgb_fixed(v, v, 255) }
        2 => { let v = fade(step); rgb_fixed(v, v, 255) }
        _ => { let v = fade(63 - step); rgb_fixed(255, v, v) }
    }
}

fn fade(step: u16) -> u8 {
    ((step * 255) / 63) as u8
}

fn wheel(mut pos: u8) -> RGB8 {
    pos = 255u8.wrapping_sub(pos);
    if pos < 85 {
        rgb_fixed(255u8.wrapping_sub(pos * 3), 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        rgb_fixed(0, pos * 3, 255u8.wrapping_sub(pos * 3))
    } else {
        let pos = pos - 170;
        rgb_fixed(pos * 3, 255u8.wrapping_sub(pos * 3), 0)
    }
}