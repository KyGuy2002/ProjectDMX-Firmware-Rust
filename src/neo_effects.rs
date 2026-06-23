use smart_leds::RGB8;

// pub fn rainbow<const N: usize>(leds: &mut [RGB8; N], offset: u8) {
//     for i in START_LED..(START_LED + NUM_LEDS) {
//         let local_i = i - START_LED;
//         let color_index = ((local_i * 256 / NUM_LEDS) as u8).wrapping_add(offset);

//         leds[i] = wheel(color_index);
//     }
// }

pub fn fourth_july<const N: usize>(
    leds: &mut [RGB8; N],
    offset: u8,
    startLed: usize,
    length: usize,
) {
    for i in startLed..(startLed + length) {
        let local_i = i - startLed;

        let pos = (((local_i * 256) / length) as u8)
            .wrapping_add(offset);

        leds[i] = patriotic_wheel(pos);
    }
}

fn patriotic_wheel(pos: u8) -> RGB8 {
    let section = pos / 64;
    let step = (pos % 64) as u16;

    match section {
        // Red -> White
        0 => {
            let v = fade(step);
            rgb_fixed(255, v, v)
        }

        // White -> Blue
        1 => {
            let v = fade(63 - step);
            rgb_fixed(v, v, 255)
        }

        // Blue -> White
        2 => {
            let v = fade(step);
            rgb_fixed(v, v, 255)
        }

        // White -> Red
        _ => {
            let v = fade(63 - step);
            rgb_fixed(255, v, v)
        }
    }
}

fn fade(step: u16) -> u8 {
    ((step * 255) / 63) as u8
}

// Your LEDs have red and green swapped.
// Call with logical RGB values.
fn rgb_fixed(r: u8, g: u8, b: u8) -> RGB8 {
    RGB8::new(g, r, b)
}

fn wheel(pos: u8) -> RGB8 {
    let pos = 255u8.wrapping_sub(pos);

    if pos < 85 {
        RGB8::new(
            255u8.wrapping_sub(pos * 3),
            0,
            pos * 3,
        )
    } else if pos < 170 {
        let pos = pos - 85;
        RGB8::new(
            0,
            pos * 3,
            255u8.wrapping_sub(pos * 3),
        )
    } else {
        let pos = pos - 170;
        RGB8::new(
            pos * 3,
            255u8.wrapping_sub(pos * 3),
            0,
        )
    }
}