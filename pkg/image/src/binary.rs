use common::ceil_div;

/// An image consisting only of two colors (black and white) where each color is
/// internally encoded in 1 bit.
pub struct BinaryImage {
    /// Bit encoded color data. Bits are encoding from MSB to LSB starting with
    /// bits at y=0 x=0. The second encoded pixel is at y=0 x=1.
    data: Vec<u8>,

    width: usize,

    height: usize,
}

impl BinaryImage {
    pub fn zero(height: usize, width: usize) -> Self {
        let n = ceil_div(height * width, 8);

        Self {
            data: vec![0; n],
            width,
            height,
        }
    }

    /// Gets the value of a single pixel.
    pub fn get(&self, y: usize, x: usize) -> u8 {
        let offset = y * self.width * 8 + 8 * x;
        let byte_offset = offset / 8;
        let bit_offset = offset % 8;

        let v = self.data[byte_offset];

        (v >> (7 - bit_offset)) & 0b1
    }

    pub fn set(&mut self, y: usize, x: usize, value: u8) {
        assert!(value & 1 == value);

        let offset = y * self.width + x;
        let byte_offset = offset / 8;
        let bit_offset = offset % 8;

        let v = &mut self.data[byte_offset];

        // Clear any old value.
        let mask = !(1 << (7 - bit_offset));
        *v &= mask;

        *v |= value << (7 - bit_offset);
    }

    pub fn row_data(&self, y: usize) -> &[u8] {
        // Rows must be delimited by different bytes.
        assert_eq!(self.width % 8, 0);

        let start_offset = (y * self.width) / 8;
        let end_offset = ((y + 1) * self.width) / 8;

        &self.data[start_offset..end_offset]
    }
}
