// TODO: Move this out of the jpeg module.

// Based on T.871
// TODO: This is highly parallelizable (ideally do in CPU cache when decoding
// MCUs)
pub fn jpeg_ycbcr_to_rgb(inputs: &mut [u8]) {
    let clamp = |v: f32| -> u8 { v.round().max(0.0).min(255.0) as u8 };

    for tuple in inputs.chunks_exact_mut(3) {
        let y = tuple[0] as f32;
        let cb = tuple[1] as f32;
        let cr = tuple[2] as f32;

        // TODO: Pre-subtract 128

        let r = y + 1.402 * (cr - 128.0);
        let g = y - 0.3441 * (cb - 128.0) - 0.7141 * (cr - 128.0);
        let b = y + 1.772 * (cb - 128.0);

        tuple[0] = clamp(r);
        tuple[1] = clamp(g);
        tuple[2] = clamp(b);
    }
}

const SCALE: i32 = 16384;

const fn to_fixed_point(v: f32) -> i32 {
    (v * (SCALE as f32)) as i32
}

const Y_R: i32 = to_fixed_point(0.299);
const Y_G: i32 = to_fixed_point(0.587);
const Y_B: i32 = to_fixed_point(0.114);

const CB_R: i32 = to_fixed_point(-0.1687);
const CB_G: i32 = to_fixed_point(0.3313);
const CB_B: i32 = to_fixed_point(0.5);

const CR_R: i32 = to_fixed_point(0.5);
const CR_G: i32 = to_fixed_point(0.4187);
const CR_B: i32 = to_fixed_point(0.0813);

const C_BIAS: i32 = to_fixed_point(128.0);

#[inline(never)]
pub fn jpeg_rgb_to_ycbcr(inputs: &mut [u8]) {
    let clamp = |v: i32| -> u8 {
        let mut v = (v + (SCALE / 2)) / SCALE;

        if std::intrinsics::unlikely(v < 0) {
            v = 0;
        }
        if std::intrinsics::unlikely(v > 255) {
            v = 255;
        }

        v as u8
    };

    for tuple in inputs.chunks_exact_mut(3) {
        let r = tuple[0] as i32;
        let g = tuple[1] as i32;
        let b = tuple[2] as i32;

        let y = Y_R * r + Y_G * g + Y_B * b;
        let cb = CB_R * r - CB_G * g + CB_B * b + C_BIAS;
        let cr = CR_R * r - CR_G * g - CR_B * b + C_BIAS;

        tuple[0] = clamp(y);
        tuple[1] = clamp(cb);
        tuple[2] = clamp(cr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_conversion() {
        let mut a = [128, 64, 32];
        jpeg_rgb_to_ycbcr(&mut a);
        jpeg_ycbcr_to_rgb(&mut a);

        // assert_eq!(&a, &[128, 64, 32]);
    }
}
