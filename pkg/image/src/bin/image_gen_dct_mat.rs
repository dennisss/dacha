// Computes the value of the DCT2_MAT_8X8 constant

use core::f32::consts::PI;

use math::matrix::Matrix8f;

fn dct_mat8x8() -> Matrix8f {
    let mut out = Matrix8f::zero();
    let N = 8;
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
}

fn main() {
    let mat = dct_mat8x8();

    for i in 0..mat.rows() {
        let mut line = String::new();
        for j in 0..mat.cols() {
            line.push_str(&format!("{}, ", mat[(i, j)]));
        }

        println!("{}", line);
    }

    println!("=====");
    println!("TRANSPOSE:",);

    for i in 0..mat.rows() {
        let mut line = String::new();
        for j in 0..mat.cols() {
            line.push_str(&format!("{}, ", mat[(j, i)]));
        }

        println!("{}", line);
    }
}
