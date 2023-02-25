use crate::{Color, Colorspace, Image};

impl Image<u8> {
    pub fn copy_from_rgb888(&mut self, buffer: &[u8]) {
        assert_eq!(self.colorspace, Colorspace::RGB);

        for y in 0..self.height() {
            for x in 0..self.width() {
                let i = (y * self.width() + x) * 3;

                let r = buffer[i];
                let g = buffer[i + 1];
                let b = buffer[i + 2];

                // NOTE: It is little endian.
                self.set(y, x, &Color::rgb(b, g, r));
            }
        }
    }
}
