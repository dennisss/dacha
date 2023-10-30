use crate::{Color, Colorspace, Image};

impl Image<u8> {
    /// RGB888 : Single plane of interleaved R G B 8-bit values.
    /// e.g. Bytes are [ R1, G1, B1, R2, G2, B2, ...  ]
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

    /// YUV420 : 3 planes concatenated sequentially.
    /// - Plane 0: 'Y' : 8-bit values subsampled at 1x1 boxes.
    /// - Plane 1: 'Cb' : 8-bit values subsamples in 2x2 boxes.
    /// - Plane 2: 'Cr' : Similar to plane 1.
    pub fn copy_from_yuv420(&mut self, buffer: &[u8]) {
        // TODO: Need to also get the
        todo!()
    }
}
