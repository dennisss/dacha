#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

/*
Eink Window:
- 250 width x 122 height

Bitmaps:
- 24 width x 55 height

So we can render:
- 10 characters wide
- 2 characters high

*/




pub struct BitmapImageRef<'a> {
    width: usize,
    height: usize,
    data: &'a [u8]
}


include!(concat!(env!("OUT_DIR"), "/generated.rs"));





pub struct DisplayBuffer {
    // 16 bytes per row * 250 rows = 4000 bytes
    pub buffer: [u8; 4000],
}

impl DisplayBuffer {
    /// Creates a new display buffer initialized to all white (0xFF).
    /// E-ink displays generally expect 1 for white and 0 for black.
    pub fn new() -> Self {
        Self { buffer: [0xFF; 4000] }
    }

    /// Clears the display to all white.
    pub fn clear(&mut self) {
        self.buffer.fill(0xFF);
    }

    /// Draws a single pixel using logical landscape coordinates (250x122).
    pub fn draw_pixel(&mut self, x: usize, y: usize, is_white: bool) {
        // Bounds check for the logical landscape orientation
        if x >= 250 || y >= 122 {
            return;
        }

        // Rotate logical coordinates 90 degrees clockwise to physical portrait coordinates
        let phys_x = 121 - y;
        let phys_y = x;

        // Map the physical coordinates to the 1D buffer
        // 16 bytes per physical row (122 pixels fit into 16 bytes: 16 * 8 = 128)
        let byte_idx = (phys_x / 8) + (phys_y * 16);
        let bit_idx = 7 - (phys_x % 8); // MSB is the leftmost pixel

        if is_white {
            self.buffer[byte_idx] |= 1 << bit_idx; // Set bit to 1 (White)
        } else {
            self.buffer[byte_idx] &= !(1 << bit_idx); // Clear bit to 0 (Black)
        }
    }

    /// Draws a bitmap glyph starting at the specified logical top-left (x, y).
    pub fn draw_bitmap(&mut self, start_x: usize, start_y: usize, glyph: &BitmapImageRef) {
        // Calculate the number of bytes per row in the glyph array, 
        // assuming standard tight packing padded to the byte boundary per row.
        let glyph_row_bytes = (glyph.width + 7) / 8;

        for glyph_y in 0..glyph.height {
            for glyph_x in 0..glyph.width {
                let draw_x = start_x + glyph_x;
                let draw_y = start_y + glyph_y;

                // Stop drawing this specific pixel if it falls off the logical screen
                if draw_x >= 250 || draw_y >= 122 {
                    continue;
                }

                // Extract the pixel from the glyph's byte array
                let data_byte_idx = (glyph_y * glyph_row_bytes) + (glyph_x / 8);
                let data_bit_idx = 7 - (glyph_x % 8);
                let is_white = (glyph.data[data_byte_idx] & (1 << data_bit_idx)) != 0;

                self.draw_pixel(draw_x, draw_y, is_white);
            }
        }
    }

    pub fn draw_text(&mut self, text: &str) {

        let mut x = 0;
        let mut y = 0;

        let mut height = 0;

        for c in text.chars() {
            if c == '\n' {
                y += height;
                x = 0;
                height = 0;
                continue;
            }

            let mut char_code = c as u32;
            if char_code > 0xff {
                char_code = 0xff;
            }

            if FONT_BITMAP_LIST[char_code as usize].is_none() {
                char_code = '?' as u32;
            }

            let b = FONT_BITMAP_LIST[char_code as usize].as_ref().unwrap();
            self.draw_bitmap(x, y, b);

            x += b.width;
            height = b.height;
        }
    }
}