use std::time::{Instant, Duration};
use std::marker::PhantomData;
use std::collections::{HashSet, HashMap};

use common::hash::FastHasherBuilder;
use image::{Image, Colorspace, Color};
use math::array::{Array, KernelEdgeMode};
use math::matrix::{Vector2f, vec2f, MatrixXf, Matrix2f, VectorXf};

use crate::checkerboard::utils::*;

const CONE_FILTER_RADIUS: usize = 5;

const SURFACE_FIT_RADIUS: usize = 5;

const NUM_SURFACE_FITTING_ITERS: usize = 5;

/// Maximum number of pixels away from the original unrefined corner that we are allowed to
/// move.
const MAX_DRIFT: f32 = 4.0;

const MIN_SHIFT_PER_ITERATION: f32 = 0.01;

// TODO: Make sure we are using the correct pixel center coordinates.


/*
We are fitting an image patch around the current point to the polynomial:
    f(x,y) = a0 x^2 + a1 x y + a2 y^2 + a3 x + a4 y + a5

where x=0, y=0 is the current point.

The saddle point (checkerboard corner) should be at the position where
the gradients of this polynomial are 0. The gradients are:

    df/dx = 2 a0 x + a1 y + a3
    df/dy = a1 x + 2 a2 y + a4

So if we set this to zero:
    2 a0 x + a1 y + a3 = 0
    a1 x + 2 a2 y + a4 = 0

And rearrange a bit:
    2 a0 x +   a1 y  = -a3
      a1 x + 2 a2 y  = -a4

we can solve this for x/y to get the position of the corner.
*/
pub struct SubpixelCornerRefiner {
    filtered_image: Image1cRef<Vec<f32>, f32>,
    poly_mat_inv: MatrixXf,
}

impl SubpixelCornerRefiner {
    pub fn new(image: &Array<f32>) -> Self {

        let filter_kernel = Self::create_cone_filter();

        // NOTE: We don't filter the image directly. Instead we apply the kernel
        // as weights to the polynomial fitting. 
        // let filtered_image = image.cross_correlate(&filter_kernel, KernelEdgeMode::Mirror);

        let k = SURFACE_FIT_RADIUS;
        let window_size = 2 * k + 1;

        let mut poly_mat = MatrixXf::zero_with_shape(window_size  * window_size, 6);

        let mut idx = 0;
        for i in (-(k as isize))..((k + 1) as isize) {
            for j in (-(k as isize))..((k + 1) as isize) {
                let y = i as f32;
                let x = j as f32;

                poly_mat[(idx, 0)] = x * x;
                poly_mat[(idx, 1)] = x * y;
                poly_mat[(idx, 2)] = y * y;
                poly_mat[(idx, 3)] = x;
                poly_mat[(idx, 4)] = y;
                poly_mat[(idx, 5)] = 1.0;

                idx += 1;
            }
        }

        let mut weights = MatrixXf::zero_with_shape(window_size*window_size, window_size*window_size);
        for i in 0..weights.rows() {
            weights[(i, i)] = filter_kernel[i];
        }

        // Weighted least squares.
        let poly_mat_inv = {
            (poly_mat.transpose() * &weights * &poly_mat).inverse() * poly_mat.transpose() * weights
            // pinv(&poly_mat)
        };

        Self {
            filtered_image: Image1cRef::new(
                image.data.clone(), image.shape[0], image.shape[1] 
            ),

            // filtered_image: Image1cRef::new(
            //     filtered_image.data, filtered_image.shape[0], filtered_image.shape[1]
            // ),
            poly_mat_inv
        }
    }

    pub fn refine_corner(&self, pt: &Vector2f) -> Option<Vector2f> {

        let mut refined_pt = pt.clone();
        for _ in 0..NUM_SURFACE_FITTING_ITERS {

            let image_patch = match self.get_image_patch(&refined_pt) {
                Some(v) => v,
                None => return None
            };

            let poly_params = &self.poly_mat_inv * &image_patch;

            let a = Matrix2f::from_slice(&[
                2.0 * poly_params[0], poly_params[1],
                poly_params[1], 2.0 * poly_params[2],
            ]);
            let b = Vector2f::from_slice(&[
                -poly_params[3],
                -poly_params[4],
            ]);

            if a.determinant() > -0.0001 {
                return None;
            }

            let a_inv = a.inverse();

            let delta = a_inv * b;

            refined_pt += delta.clone();

            if refined_pt[0].is_nan() || refined_pt[1].is_nan() {
                return None;
            }

            if (&refined_pt - pt).norm() > MAX_DRIFT {
                return None;
            }

            if delta.norm() < MIN_SHIFT_PER_ITERATION {
                break;
            }
        }

        Some(refined_pt)
    }

    fn create_cone_filter() -> Array<f32> {
        let r = CONE_FILTER_RADIUS;
        let kernel_size = 2 * r + 1;

        let mut kernel = Array {
            data: vec![0.0f32; kernel_size * kernel_size],
            shape: vec![kernel_size, kernel_size]
        };

        let mut sum = 0.0;

        for i in 0..kernel_size {
            for j in 0..kernel_size {
                let v = (
                    (r as f32) + 1.0 - (squared((r as f32) - (i as f32)) + squared((r as f32) - (j as f32))).sqrt()
                ).max(0.0);

                kernel[&[i, j][..]] = v;
                sum += v;
            }
        }

        for i in 0..kernel_size {
            for j in 0..kernel_size {
                kernel[&[i, j][..]] /= sum;
            }
        }

        kernel
    }

    /// Returns None if the patch would get outside the image.
    fn get_image_patch(&self, center_pt: &Vector2f) -> Option<VectorXf> {
        let k = SURFACE_FIT_RADIUS;
        let window_size = 2 * k + 1;


        let mut out = VectorXf::zero_with_shape(window_size * window_size, 1);
        
        // Bounds check for the entire image patch.
        {
            let width = self.filtered_image.width() as isize;
            let height = self.filtered_image.height() as isize;

            let x = center_pt.x() as isize;
            let y = center_pt.y() as isize;
            let k = k as isize;

            if x - k < 0 || x + k + 1 >= width {
                return None;
            }

            if y - k < 0 || y + k + 1 >= height {
                return None;
            }

        }

        let mut idx = 0;
        for i in (-(k as isize))..((k + 1) as isize) {
            for j in (-(k as isize))..((k + 1) as isize) {
                let y = center_pt.y() + (i as f32);
                let x = center_pt.x() + (j as f32);

                out[idx] = self.get_subpixel(&vec2f(x, y));

                idx += 1;
            }
        }

        Some(out)
    }

    fn get_subpixel(&self, pt: &Vector2f) -> f32 {
        // TODO: This initial alpha calculation math can be cached across the entire image patch.

        let x1 = pt.x().floor() as usize;
        let x2 = x1 + 1;
        let x1_alpha = 1.0 - pt.x().fract();

        let y1 = pt.y().floor() as usize;
        let y2 = y1 + 1;
        let y1_alpha = 1.0 - pt.y().fract();

        let row1 = interp(
            self.filtered_image.get(y1, x1),
            self.filtered_image.get(y1, x2),
            x1_alpha
        );
        let row2 = interp(
            self.filtered_image.get(y2, x1),
            self.filtered_image.get(y2, x2),
            x1_alpha
        );

        interp(
            row1,
            row2,
            y1_alpha
        )
    }

}

// TODO: Dedup this.
fn interp(a: f32, b: f32, a_alpha: f32) -> f32 {
    a * a_alpha + b * (1.0 - a_alpha)
}

fn squared(v: f32) -> f32 {
    v * v
}
