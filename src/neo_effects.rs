// src/neo_effects.rs

use smart_leds::RGB8;
use crate::config::{PixelMeta, NUM_LEDS};
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

        _ => RGB8::default(),
    }
}

/// Applies modifications directly onto background arrays using spatial triggers
pub fn apply_top_effect(
    id: u8,
    offset: u8,
    bg_color: RGB8,
    meta: &PixelMeta,
) -> RGB8 {
    if !meta.is_valid {
        return RGB8::default();
    }

    match id {
        // 1..4 Horizontal panning sweeps
        1 | 2 | 3 | 4 => {
            let x_val = meta.x;
            let target_pos = offset; 
            let range = 32u8; // Pulse dimension limit

            let in_beam = match id {
                1 => x_val.saturating_sub(target_pos) < range,                          // Left to Right
                2 => (255 - x_val).saturating_sub(target_pos) < range,                  // Right to Left
                3 => (128u8.saturating_sub(x_val)).abs_diff(target_pos) < (range / 2),   // Outer to Inner
                4 => (x_val.abs_diff(128)).saturating_sub(target_pos) < (range / 2),    // Inner to Outer
                _ => false,
            };

            if in_beam {
                rgb_fixed(255, 255, 255) // Bright White beam strike
            } else {
                // Dim existing background by 75%
                RGB8 { r: bg_color.r >> 2, g: bg_color.g >> 2, b: bg_color.b >> 2 }
            }
        }

        // 5..6 Vertical panning sweeps
        5 | 6 => {
            let y_val = meta.y;
            let target_pos = offset;
            let range = 32u8;

            let in_beam = match id {
                5 => y_val.saturating_sub(target_pos) < range,         // Top to Bottom
                6 => (255 - y_val).saturating_sub(target_pos) < range, // Bottom to Top
                _ => false,
            };

            if in_beam {
                rgb_fixed(255, 255, 255)
            } else {
                RGB8 { r: bg_color.r >> 2, g: bg_color.g >> 2, b: bg_color.b >> 2 }
            }
        }

        // 7: Segmented Letter Flash Strobe
        7 => {
            let pseudo_rand = (meta.letter_id as u8).wrapping_mul(79).wrapping_add(offset >> 2);
            if (pseudo_rand % 3) == 0 {
                bg_color // Maintain original color profile
            } else {
                RGB8::default() // Flash blank down
            }
        }

        _ => bg_color, // Bypass modifier without transformations
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