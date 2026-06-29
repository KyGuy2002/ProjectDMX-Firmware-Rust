pub const NUM_LEDS: usize = 200;

#[derive(Clone, Copy, Debug)]
pub struct PixelMeta {
    pub index: usize,
    pub letter_id: usize, // 0='U', 1='S', 2='A', 3='2', 4='5', 5='0'
    pub x: u8,            // 0 (Far Left) to 255 (Far Right)
    pub y: u8,            // 0 (Top) to 255 (Bottom)
    pub is_valid: bool,   // true = lit, false = unmapped/skipped spacer wire
}

pub struct SegmentRule {
    pub start_idx: usize,
    pub length: usize,
    pub letter_id: usize,
    pub min_x: u8,
    pub max_x: u8,
    pub min_y: u8,
    pub max_y: u8,
    pub force_invalid: bool,
    pub invert_y: bool, // Explicit flag instead of guessing by letter_id
}

impl SegmentRule {
    /// Standard vertical or horizontal line where the wiring goes forward (top-to-bottom / left-to-right)
    pub const fn line(start_idx: usize, length: usize, letter_id: usize, min_x: u8, max_x: u8, min_y: u8, max_y: u8) -> Self {
        Self {
            start_idx,
            length,
            letter_id,
            min_x,
            max_x,
            min_y,
            max_y,
            force_invalid: false,
            invert_y: false,
        }
    }

    /// Standard line or segment where the physical wire is traveling backward (bottom-to-top)
    pub const fn line_inverted(start_idx: usize, length: usize, letter_id: usize, min_x: u8, max_x: u8, min_y: u8, max_y: u8) -> Self {
        Self {
            start_idx,
            length,
            letter_id,
            min_x,
            max_x,
            min_y,
            max_y,
            force_invalid: false,
            invert_y: true,
        }
    }

    /// A diagonal slash. Maps explicitly from (x1, y1) to (x2, y2) in absolute spatial coordinates
    pub const fn diagonal(start_idx: usize, length: usize, letter_id: usize, x1: u8, y1: u8, x2: u8, y2: u8) -> Self {
        Self {
            start_idx,
            length,
            letter_id,
            min_x: x1,
            max_x: x2,
            min_y: y1,
            max_y: y2,
            force_invalid: false,
            invert_y: false, // Keep spatial direction absolute
        }
    }

    pub const fn single_pixel(idx: usize, letter_id: usize, x: u8, y: u8) -> Self {
        Self {
            start_idx: idx,
            length: 1,
            letter_id,
            min_x: x,
            max_x: x,
            min_y: y,
            max_y: y,
            force_invalid: false,
            invert_y: false,
        }
    }

    pub const fn skip(start_idx: usize, length: usize) -> Self {
        Self {
            start_idx,
            length,
            letter_id: 0,
            min_x: 0,
            max_x: 0,
            min_y: 0,
            max_y: 0,
            force_invalid: true,
            invert_y: false,
        }
    }
}

// -------------------------------------------------------------
// SETTINGS FILE CONFIGURATION BLOCK
// -------------------------------------------------------------
// Define your physical straight line blocks here. 
// Any index NOT covered here automatically drops into the skip/blank pool.

// U: 0   S: 1   A: 2   L2: 3   L5: 4   L0: 5

const CONFIG_SEGMENTS: &[SegmentRule] = &[


    // --- USA (Left Side of layout) ---
    SegmentRule::line(8, 19, 0, 10, 35, 0, 255),  // 'U' Line
    SegmentRule::line(41, 18, 1, 45, 75, 0, 255), // 'S' Line
    SegmentRule::line(72, 22, 2, 85, 115, 0, 255),// 'A' Line

    // --- 250 Section ---

    // ----- Letter: 2 (top left to bottom right order)
    SegmentRule::single_pixel(108, 3, 170, 50),
    SegmentRule::single_pixel(109, 3, 173, 25),
    SegmentRule::line(110, 3, 3, 176, 185, 0, 0), // Top flat 3
    SegmentRule::single_pixel(113, 3, 188, 25),
    SegmentRule::single_pixel(114, 3, 191, 50),
    SegmentRule::diagonal(115, 7, 3, 191, 75, 170, 255), // Center diag
    SegmentRule::line(122, 4, 3, 174, 191, 255, 255), // Bottom flat

    // ----- Letter: 5 (bottom left to top right order)
    SegmentRule::single_pixel(143, 4, 192, 255-50),
    SegmentRule::single_pixel(144, 4, 195, 255-25),
    SegmentRule::line(145, 2, 4, 198, 201, 255, 255), // Bottom flat 2
    SegmentRule::single_pixel(147, 4, 204, 255-25),
    SegmentRule::line(148, 3, 4, 207, 207, 255-50, 255-100), // Vertical right 3
    SegmentRule::single_pixel(151, 4, 204, 255-125),
    SegmentRule::line(152, 2, 4, 201, 198, 255-150, 255-150), // Middle flat 2
    SegmentRule::single_pixel(154, 4, 195, 255-175),
    SegmentRule::line(155, 2, 4, 192, 192, 50, 25), // Vertical left 2 (not incl top pixel)
    SegmentRule::line(157, 5, 4, 192, 214, 0, 0), // Top flat 5

    // ----- Letter: 0 (clockwise - string starts topleft - code below starts with bottom of leftmost vertical line)
    SegmentRule::line(191, 4, 5, 215, 215, 255-60, 60), // Left vertical 4
    SegmentRule::single_pixel(177, 5, 217, 30),
    SegmentRule::line(178, 3, 5, 219, 227, 0, 0), // Top flat 3
    SegmentRule::single_pixel(181, 5, 232, 30),
    SegmentRule::line(182, 4, 5, 236, 236, 60, 255-60), // Right vertical 4
    SegmentRule::single_pixel(186, 5, 232, 255-30),
    SegmentRule::line(187, 3, 5, 227, 219, 255, 255), // Bottom flat 3
    SegmentRule::single_pixel(190, 5, 217, 255-30),


];

pub const fn get_layout_map() -> [PixelMeta; NUM_LEDS] {
    let mut map = [PixelMeta {
        index: 0,
        letter_id: 0,
        x: 0,
        y: 0,
        is_valid: false,
    }; NUM_LEDS];

    let mut i = 0;
    while i < NUM_LEDS {
        map[i].index = i;
        let mut matched = false;
        let mut seg_idx = 0;

        while seg_idx < CONFIG_SEGMENTS.len() {
            let seg = &CONFIG_SEGMENTS[seg_idx];
            
            if i >= seg.start_idx && i < (seg.start_idx + seg.length) {
                if seg.force_invalid {
                    break;
                }

                let local_offset = i - seg.start_idx;
                
                let step = if seg.length > 1 {
                    (local_offset * 255) / (seg.length - 1)
                } else {
                    0
                };
                
                let delta_x = (seg.max_x as i16 - seg.min_x as i16) as i32;
                let delta_y = (seg.max_y as i16 - seg.min_y as i16) as i32;

                map[i].letter_id = seg.letter_id;
                map[i].x = (seg.min_x as i32 + ((step as i32 * delta_x) / 255)) as u8;
                
                if seg.length == 1 || delta_y == 0 {
                    map[i].y = seg.min_y;
                } else if !seg.invert_y {
                    // Coordinates map exactly forward down physical space
                    map[i].y = (seg.min_y as i32 + ((step as i32 * delta_y) / 255)) as u8;
                } else {
                    // Physical wire goes upward, map coordinates reversed
                    map[i].y = (seg.max_y as i32 - ((step as i32 * delta_y) / 255)) as u8;
                }
                
                map[i].is_valid = true;
                matched = true;
                break;
            }
            seg_idx += 1;
        }

        if !matched {
            map[i].is_valid = false;
            map[i].x = 0;
            map[i].y = 0;
            map[i].letter_id = 0;
        }

        i += 1;
    }

    map
}