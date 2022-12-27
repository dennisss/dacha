use executor::bundle::TaskBundle;
use math::array::Array;
use math::matrix::Vector3u;

use crate::{Colorspace, Image};

impl Image<u8> {
    /// Performs a bilinear resize.
    pub fn resize(&self, height: usize, width: usize) -> Self {
        let mut data = Array::new(&[height, width, self.channels()]);

        let x_scale = (self.width() as f32) / (width as f32);
        let y_scale = (self.height() as f32) / (height as f32);

        for y in 0..height {
            for x in 0..width {
                let old_xf = (x as f32) * x_scale;
                let old_yf = (y as f32) * y_scale;

                let old_x = std::cmp::min(old_xf as usize, self.width() - 1);
                let old_y = std::cmp::min(old_yf as usize, self.height() - 1);

                let dx = old_xf - (old_x as f32);
                let dy = old_yf - (old_y as f32);

                let a = (1. - dx) * (1. - dy);
                let b = dx * (1. - dy);
                let c = (1. - dx) * dy;
                let d = dx * dy;

                for i in 0..self.channels() {
                    data[&[y, x, i][..]] = (a * self.array[&[old_y, old_x, i][..]] as f32
                        + b * self.array[&[old_y, old_x + 1, i][..]] as f32
                        + c * self.array[&[old_y + 1, old_x, i][..]] as f32
                        + d * self.array[&[old_y + 1, old_x + 1, i][..]] as f32)
                        as u8;
                }
            }
        }

        Self {
            array: data,
            colorspace: self.colorspace,
        }
    }

    /// This is essentially a convolution based resize.
    /// NOTE: Assumes 3 color channels.
    pub async fn downsample(&self, output: &mut Image<u8>) {
        assert_eq!(self.width() % output.width(), 0);
        assert_eq!(self.height() % output.height(), 0);

        let x_scale = self.width() / output.width();
        let y_scale = self.height() / output.height();
        let xy_scale = x_scale * y_scale;

        let mut cumulative = Image::<u16>::zero(output.height(), output.width(), Colorspace::RGB);

        //        let mut x = 0;
        //        let mut y = 0;

        let input_width = self.width();
        let output_width = output.width();

        let num_tasks = 4;
        let output_rows_interval = common::ceil_div(output.height(), num_tasks);

        let input_row_size = input_width * 3;
        let output_row_size = output_width * 3;

        let input_chunks = self
            .array
            .data
            .chunks(4 * output_rows_interval * input_row_size);

        let output_chunks = cumulative
            .array
            .data
            .chunks_mut(output_rows_interval * output_row_size);

        let mut bundle = TaskBundle::new();

        for (input, output) in input_chunks.into_iter().zip(output_chunks.into_iter()) {
            bundle.add(async move {
                let mut x = 0;
                let mut y = 0;

                for (mut i, v) in input.chunks_exact(3).enumerate() {
                    let xo = x >> 2; // / x_scale;
                    let yo = y >> 2; // / y_scale;

                    let base = 3 * (output_width * yo + xo);
                    for c in 0..3 {
                        output[base + c] += v[c] as u16;
                    }

                    x += 1;
                    if x == input_width {
                        x = 0;
                        y += 1;
                    }
                }
            });
        }

        bundle.join().await;

        /*
        for (mut i, v) in self.array.data.chunks_exact(3).enumerate() {
            let xo = x >> 2; // / x_scale;
            let yo = y >> 2; // / y_scale;

            let base = 3 * (output_width * yo + xo);
            for c in 0..3 {
                cumulative.array.data[base + c] += v[c] as u16;
            }

            x += 1;
            if x == input_width {
                x = 0;
                y += 1;
            }
        }
        */

        for (v_in, v_out) in cumulative
            .array
            .data
            .iter()
            .zip(output.array.data.iter_mut())
        {
            *v_out = ((*v_in as usize) >> 4) as u8 //  / xy_scale) as u8;
        }

        /*
        for y in 0..output.height() {
            for x in 0..output.width() {
                let mut sum = Vector3u::zero();
                for y_i in 0..y_scale {
                    for x_i in 0..x_scale {
                        for i in 0..3 {
                            sum[i] += self[(y * y_scale + y_i, x * x_scale + x_i, i)] as usize;
                        }
                    }
                }

                for i in 0..3 {
                    output[(y, x, i)] = (sum[i] / xy_scale) as u8;
                }
            }
        }
        */
    }
}
