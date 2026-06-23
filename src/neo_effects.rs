use smart_leds::RGB8;

pub fn rainbow<const N: usize>(leds: &mut [RGB8; N], offset: u8) {
    for i in 0..N {
        let color_index = ((i * 256 / N) as u8).wrapping_add(offset);
        leds[i] = wheel(color_index);
    }
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