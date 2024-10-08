use core::f32::consts::PI;
use std::ops::{Index, IndexMut};

use math::matrix::*;

use crate::format::jpeg::constants::*;

/*
TODO: Read the 'Practical Fast 1-D DCT Algorithms with 11 Multiplications' paper
*/

/*
type Matrix8f = [[f32; BLOCK_DIM]; BLOCK_DIM];

trait MatrixLike {
    fn index(&self, i: usize, j: usize) -> &f32;
}

trait MatrixLikeMut {
    fn index_mut(&mut self, i: usize, j: usize) -> &mut f32;
}

impl MatrixLike for Matrix8f {
    fn index(&self, i: usize, j: usize) -> &f32 {
        &self[i][j]
    }
}

impl MatrixLikeMut for Matrix8f {
    fn index_mut(&mut self, i: usize, j: usize) -> &mut f32 {
        &mut self[i][j]
    }
}

pub struct MatrixTranspose<'a, T> {
    inner: &'a T,
}

impl<'a, T: MatrixLike> MatrixLike for MatrixTranspose<'a, T> {
    fn index(&self, i: usize, j: usize) -> &f32 {
        unimplemented!()
    }
}

// c = a' * b
fn matmul(a: &Matrix8f, b: &Matrix8f, c: &mut Matrix8f) {
    for i in 0..8 {
        for j in 0..8 {
            let c_ij = &mut c[i][j];
            *c_ij = 0.0;
            for k in 0..8 {
                *c_ij += a[k][i] * b[k][j];
            }
        }
    }
}

*/

/*
// c = a * b
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn matmul(a: &Matrix8f, b: &Matrix8f, c: &mut Matrix8f) {
    for i in 0..8 {
        for j in 0..8 {
            let c_ij = &mut c[(i, j)];
            *c_ij = 0.0;
            for k in 0..8 {
                *c_ij += a[(i, k)] * b[(k, j)];
            }
        }
    }
}
*/

// Computed by the image_gen_dct_mat binary.
#[rustfmt::skip]
pub static DCT2_MAT_8X8: &[f32; BLOCK_SIZE] = &[
    0.35355338, 0.35355338, 0.35355338, 0.35355338, 0.35355338, 0.35355338, 0.35355338, 0.35355338, 
    0.49039263, 0.4157348, 0.2777851, 0.09754512, -0.09754516, -0.27778518, -0.41573483, -0.49039266, 
    0.46193975, 0.19134171, -0.19134176, -0.4619398, -0.46193975, -0.19134156, 0.1913418, 0.46193978, 
    0.4157348, -0.09754516, -0.49039266, -0.277785, 0.27778503, 0.49039263, 0.097545035, -0.4157349, 
    0.35355338, -0.35355338, -0.35355332, 0.3535535, 0.35355338, -0.35355362, -0.35355327, 0.3535534, 
    0.2777851, -0.49039266, 0.09754521, 0.41573468, -0.4157349, -0.09754464, 0.49039266, -0.27778503, 
    0.19134171, -0.46193975, 0.46193987, -0.19134195, -0.19134192, 0.46193966, -0.46193987, 0.19134195, 
    0.09754512, -0.277785, 0.41573468, -0.4903926, 0.49039263, -0.41573507, 0.27778557, -0.097544834,
];

// Computed by the image_gen_dct_mat binary.
#[rustfmt::skip]
pub static DCT2_MAT_8X8_TRANSPOSE: &[f32; BLOCK_SIZE] = &[
    0.35355338, 0.49039263, 0.46193975, 0.4157348, 0.35355338, 0.2777851, 0.19134171, 0.09754512, 
    0.35355338, 0.4157348, 0.19134171, -0.09754516, -0.35355338, -0.49039266, -0.46193975, -0.277785,
    0.35355338, 0.2777851, -0.19134176, -0.49039266, -0.35355332, 0.09754521, 0.46193987, 0.41573468,
    0.35355338, 0.09754512, -0.4619398, -0.277785, 0.3535535, 0.41573468, -0.19134195, -0.4903926, 
    0.35355338, -0.09754516, -0.46193975, 0.27778503, 0.35355338, -0.4157349, -0.19134192, 0.49039263, 
    0.35355338, -0.27778518, -0.19134156, 0.49039263, -0.35355362, -0.09754464, 0.46193966, -0.41573507, 
    0.35355338, -0.41573483, 0.1913418, 0.097545035, -0.35355327, 0.49039266, -0.46193987, 0.27778557, 
    0.35355338, -0.49039266, 0.46193978, -0.4157349, 0.3535534, -0.27778503, 0.19134195, -0.097544834, 
];

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
fn to_m256(v: &[f32; 8]) -> __m256 {
    unsafe { _mm256_loadu_ps(v.as_ptr()) }
}

fn matmul(a_mat: &Matrix8f, b_mat: &Matrix8f, c_mat: &mut Matrix8f) {
    let a = unsafe {
        std::mem::transmute::<_, &[[f32; BLOCK_DIM]; BLOCK_DIM]>(array_ref![a_mat.as_ref(), 0, 64])
    };
    let b = unsafe {
        std::mem::transmute::<_, &[[f32; BLOCK_DIM]; BLOCK_DIM]>(array_ref![b_mat.as_ref(), 0, 64])
    };
    let c = unsafe {
        std::mem::transmute::<_, &mut [[f32; BLOCK_DIM]; BLOCK_DIM]>(array_mut_ref![
            c_mat.as_mut(),
            0,
            64
        ])
    };

    matmul_impl(a, b, c);
}

/// Computes c_mat = a_mat * b_mat
#[cfg(target_arch = "x86_64")]
fn matmul_impl(
    a: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    b: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    c: &mut [[f32; BLOCK_DIM]; BLOCK_DIM],
) {
    for i in 0..8 {
        let mut c_i = unsafe { _mm256_setzero_ps() };

        for j in 0..8 {
            let a_j = unsafe { _mm256_broadcast_ss(&a[i][j]) };
            let b_i = to_m256(&b[j]);

            // let b_j = to_m256(&b[j]);
            let r = unsafe { _mm256_mul_ps(a_j, b_i) };
            c_i = unsafe { _mm256_add_ps(c_i, r) };
        }

        unsafe { _mm256_storeu_ps(c[i].as_mut_ptr(), c_i) };
    }
}

/// Computes 'c_mat = a_mat * b_mat' and performs elementwise scaling of every
/// element of 'c_mat' by the correspoding element in 'scale'
#[cfg(target_arch = "x86_64")]
fn matmul_and_scale_impl(
    a: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    b: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    scale: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    c: &mut [[f32; BLOCK_DIM]; BLOCK_DIM],
) {
    for i in 0..8 {
        let mut c_i = unsafe { _mm256_setzero_ps() };

        for j in 0..8 {
            let a_j = unsafe { _mm256_broadcast_ss(&a[i][j]) };
            let b_i = to_m256(&b[j]);

            // let b_j = to_m256(&b[j]);
            let r = unsafe { _mm256_mul_ps(a_j, b_i) };
            c_i = unsafe { _mm256_add_ps(c_i, r) };
        }

        // These two lines are the main difference compared to the 'matmul_impl' code.
        let scale = to_m256(&scale[i]);
        c_i = unsafe { _mm256_mul_ps(c_i, scale) };

        unsafe { _mm256_storeu_ps(c[i].as_mut_ptr(), c_i) };
    }
}

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

/// Computes c_mat = a_mat * b_mat
///
/// See also https://developer.arm.com/documentation/102467/0201/Example---matrix-multiplication?lang=en
#[cfg(target_arch = "aarch64")]
fn matmul_impl(
    a: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    b: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    c: &mut [[f32; BLOCK_DIM]; BLOCK_DIM],
) {
    for i in 0..8 {
        let mut c_i = unsafe { float32x4x2_t(vmovq_n_f32(0.0), vmovq_n_f32(0.0)) };

        // TODO: Do this once and use the lane muls
        // let a_j = unsafe { vdupq_n_f32(a[i][j]) };

        // The lane multiply intrinsic is vfmaq_lane_f32

        for j in 0..8 {
            let a_j = unsafe { vdupq_n_f32(a[i][j]) };

            let b_i = unsafe { vld1q_f32_x2((&b[j]).as_ptr()) };

            c_i.0 = unsafe { vfmaq_f32(c_i.0, a_j, b_i.0) };
            c_i.1 = unsafe { vfmaq_f32(c_i.1, a_j, b_i.1) };
        }

        unsafe { vst1q_f32_x2(c[i].as_mut_ptr(), c_i) };
    }
}

#[cfg(target_arch = "aarch64")]
fn matmul_and_scale_impl(
    a: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    b: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    scale: &[[f32; BLOCK_DIM]; BLOCK_DIM],
    c: &mut [[f32; BLOCK_DIM]; BLOCK_DIM],
) {
    for i in 0..8 {
        let mut c_i = unsafe { float32x4x2_t(vmovq_n_f32(0.0), vmovq_n_f32(0.0)) };

        // TODO: Do this once and use the lane muls
        // let a_j = unsafe { vdupq_n_f32(a[i][j]) };

        for j in 0..8 {
            let a_j = unsafe { vdupq_n_f32(a[i][j]) };

            let b_i = unsafe { vld1q_f32_x2((&b[j]).as_ptr()) };

            c_i.0 = unsafe { vfmaq_f32(c_i.0, a_j, b_i.0) };
            c_i.1 = unsafe { vfmaq_f32(c_i.1, a_j, b_i.1) };
        }

        let scale = unsafe { vld1q_f32_x2((&scale[i]).as_ptr()) };
        c_i.0 = unsafe { vmulq_f32(c_i.0, scale.0) };
        c_i.1 = unsafe { vmulq_f32(c_i.1, scale.1) };

        unsafe { vst1q_f32_x2(c[i].as_mut_ptr(), c_i) };
    }
}

#[inline(never)]
pub fn forward_dct_2d(
    input: &[u8; BLOCK_SIZE],
    output_scale: &[f32; BLOCK_SIZE],
    output: &mut [i16; BLOCK_SIZE],
) {
    let mut temp1 = [0.0f32; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        // Center around zero and convert to f32.
        temp1[i] = ((input[i] as i16) - 128) as f32;
    }

    let mut temp2 = [0.0f32; BLOCK_SIZE];

    // Output = M * X * M'
    unsafe {
        matmul_impl(
            core::mem::transmute(DCT2_MAT_8X8),
            core::mem::transmute(&temp1),
            core::mem::transmute(&mut temp2),
        );

        matmul_and_scale_impl(
            core::mem::transmute(&temp2),
            core::mem::transmute(DCT2_MAT_8X8_TRANSPOSE),
            core::mem::transmute(output_scale),
            core::mem::transmute(&mut temp1),
        );
    };

    for i in 0..BLOCK_SIZE {
        output[i] = temp1[i].round() as i16;
    }
}

// Baseline is 0.33 seconds
// Currently this runs in 0.40 seconds, so is really SLOW even for matmul
// standards.
pub fn inverse_dct_2d(input: &[i16; BLOCK_SIZE], output: &mut [i16; BLOCK_SIZE]) {
    let mut temp1 = Matrix8f::zero();
    for (i, v) in input.iter().enumerate() {
        temp1[i] = *v as f32;
    }

    // = M' * X * M
    let dct_mat = Matrix8f::from_slice(&DCT2_MAT_8X8[..]);
    let mut temp2 = dct_mat.as_transpose() * &temp1;
    matmul(&temp2, &dct_mat, &mut temp1);

    for (i, v) in temp1.as_ref().iter().enumerate() {
        output[i] = v.round() as i16;
    }

    return;

    let alpha = |v: u8| -> f32 {
        if v == 0 {
            1.0f32 / (2.0f32).sqrt() as f32
        } else {
            1.0f32
        }
    };

    let cos = |x: u8, u: u8| -> f32 { (((2.0 * (x as f32) + 1.0) * (u as f32) * PI) / 16.0).cos() };

    for i in 0..(output.len() as u8) {
        let x = i % 8;
        let y = i / 8;

        let mut sum = 0.0;
        for v in 0..8_u8 {
            for u in 0..8_u8 {
                sum += alpha(u)
                    * alpha(v)
                    * (input[(v * 8 + u) as usize] as f32)
                    * cos(x, u)
                    * cos(y, v);
            }
        }

        // TODO: The 1/4 could be a >> 2 in integer space done at the very end?
        output[i as usize] = (((1.0 / 4.0) * sum) as f32).round() as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dct_invertable() {
        let block123 = {
            let mut block = [0i16; 64];
            for i in 0..64 {
                block[i] = i as i16;
            }

            block
        };

        let block_mod2 = {
            let mut block = [0i16; 64];
            for i in 0..64 {
                if i % 2 == 0 {
                    block[i] = 50;
                }
            }

            block
        };

        let block_mod22 = {
            let mut block = [0i16; 64];
            for i in 0..64 {
                if i % 2 == 0 {
                    block[i] = 43;
                }
                if i % 4 == 0 {
                    block[i] = -60;
                }
            }

            block
        };

        let block_mod4 = {
            let mut block = [0i16; 64];
            for i in 0..64 {
                if i % 4 == 0 {
                    block[i] = 123;
                }
            }

            block
        };

        let test_cases = vec![
            [0i16; 64],
            [128i16; 64],
            [-128i16; 64],
            [-2i16; 64],
            block123,
            block_mod2,
            block_mod22,
            block_mod4,
        ];

        let mut scale = [0.0f32; BLOCK_SIZE];

        /*
        for block in test_cases {
            let mut out = [0i16; 64];
            forward_dct_2d(&block, &scale, &mut out);

            let mut out2 = [0i16; 64];
            inverse_dct_2d(&out, &mut out2);

            assert_eq!(out2, block);
        }
        */
    }
}
