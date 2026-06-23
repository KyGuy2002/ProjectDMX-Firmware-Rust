use smart_leds::RGB8;



fn wheel(pos: u8) -> RGB8 {
    let pos = 255 - pos;

    if pos < 85 {
        RGB8::new(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        RGB8::new(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        RGB8::new(pos * 3, 255 - pos * 3, 0)
    }
}