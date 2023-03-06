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

// c = a * b
#[cfg(not(target_arch = "x86_64"))]
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

// TODO: Convert this to a constant.
lazy_static! {
    static ref DCT2_MAT_8X8: Matrix8f = {
        let mut out = Matrix8f::zero(); // [[0.0_f32; BLOCK_DIM]; BLOCK_DIM];
        let N = BLOCK_DIM;
        for k in 0..N {
            for n in 0..N {
                let mut v = ((PI / (N as f32)) * ((n as f32) + (1.0 / 2.0)) * (k as f32)).cos();

                v /= 2.0;
                if k == 0 {
                    v /= (2.0_f32).sqrt()
                }

                out[(k, n)] = v;
            }
        }

        out
    };
}

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
fn to_m256(v: &[f32; 8]) -> __m256 {
    unsafe { _mm256_loadu_ps(v.as_ptr()) }
}

/// Computes c_mat = a_mat * b_mat
#[cfg(target_arch = "x86_64")]
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

// TODO: Make an AARCH64 version of thi.
// vdupq_n_f32
// vfmaq_f32

pub fn forward_dct_2d(input: &[i16; BLOCK_SIZE], output: &mut [i16; BLOCK_SIZE]) {
    let mut temp1 = Matrix8f::zero();
    for (i, v) in input.iter().enumerate() {
        temp1[i] = *v as f32;
    }

    let mut temp2 = Matrix8f::zero();

    // = M * X * M'
    let dct_mat = &*DCT2_MAT_8X8;
    matmul(dct_mat, &temp1, &mut temp2);

    temp1 = &temp2 * dct_mat.as_transpose();

    for (i, v) in temp1.as_ref().iter().enumerate() {
        output[i] = v.round() as i16;
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
    let dct_mat = &*DCT2_MAT_8X8;
    let mut temp2 = dct_mat.as_transpose() * &temp1;
    matmul(&temp2, dct_mat, &mut temp1);

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
    fn dct() {
        let mut block = [0i16; 64];
        for i in 0..64 {
            block[i] = i as i16;
        }

        let mut out = [0i16; 64];
        forward_dct_2d(&block, &mut out);

        let mut out2 = [0i16; 64];
        inverse_dct_2d(&out, &mut out2);

        assert_eq!(out2, block);
    }
}
