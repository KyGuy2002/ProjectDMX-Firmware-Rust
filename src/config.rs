pub const NUM_LEDS: usize = 200;

#[derive(Clone, Copy, Debug)]
pub struct PixelMeta {
    pub index: usize,
    pub letter_id: usize, // 0='U', 1='S', 2='A', 3='2', 4='5', 5='0'
    pub x: u8,            // 0 (Far Left) to 255 (Far Right)
    pub y: u8,            // 0 (Top) to 255 (Bottom)
    pub is_valid: bool,   // true = lit, false = unmapped/skipped spacer wire
}

/// Helper structure to quickly outline physical straight lines/segments
pub struct SegmentRule {
    pub start_idx: usize,
    pub length: usize,
    pub letter_id: usize,
    pub min_x: u8,
    pub max_x: u8,
    pub min_y: u8,
    pub max_y: u8,
}

// -------------------------------------------------------------
// SETTINGS FILE CONFIGURATION BLOCK
// -------------------------------------------------------------
// Define your physical straight line blocks here. 
// Any index NOT covered here automatically drops into the skip/blank pool.
const CONFIG_SEGMENTS: &[SegmentRule] = &[
    // --- USA (Left Side of layout) ---
    SegmentRule { start_idx: 8,   length: 19, letter_id: 0, min_x: 10,  max_x: 35,  min_y: 0,   max_y: 255 }, // 'U'
    SegmentRule { start_idx: 41,  length: 18, letter_id: 1, min_x: 45,  max_x: 75,  min_y: 0,   max_y: 255 }, // 'S'
    SegmentRule { start_idx: 72,  length: 22, letter_id: 2, min_x: 85,  max_x: 115, min_y: 0,   max_y: 255 }, // 'A'

    // --- 250 (Right Side of layout) ---
    SegmentRule { start_idx: 108, length: 18, letter_id: 3, min_x: 140, max_x: 170, min_y: 0,   max_y: 255 }, // '2'
    SegmentRule { start_idx: 143, length: 19, letter_id: 4, min_x: 180, max_x: 210, min_y: 0,   max_y: 255 }, // '5'
    SegmentRule { start_idx: 177, length: 18, letter_id: 5, min_x: 220, max_x: 250, min_y: 0,   max_y: 255 }, // '0'
];

/// Automatically populates the full layout array at compilation time.
/// Handles your lines, handles spacing transitions, and stops type mismatch errors.
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

        // Loop through configured structural lines
        while seg_idx < CONFIG_SEGMENTS.len() {
            let seg = &CONFIG_SEGMENTS[seg_idx];
            
            if i >= seg.start_idx && i < (seg.start_idx + seg.length) {
                let local_offset = i - seg.start_idx;
                
                // Calculate percentage scale step across the segment (0 to 255)
                let step = (local_offset * 255) / (seg.length - 1);
                
                let delta_x = (seg.max_x as i16 - seg.min_x as i16) as i32;
                let delta_y = (seg.max_y as i16 - seg.min_y as i16) as i32;

                map[i].letter_id = seg.letter_id;
                map[i].x = (seg.min_x as i32 + ((step as i32 * delta_x) / 255)) as u8;
                
                // Keep zigzag alternate mapping layout fluid for vertical spans
                if seg.letter_id % 2 == 0 {
                    map[i].y = (seg.min_y as i32 + ((step as i32 * delta_y) / 255)) as u8;
                } else {
                    map[i].y = (seg.max_y as i32 - ((step as i32 * delta_y) / 255)) as u8;
                }
                
                map[i].is_valid = true;
                matched = true;
                break;
            }
            seg_idx += 1;
        }

        // If index isn't in any SegmentRule, it is an unused skip/blank pixel wire
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