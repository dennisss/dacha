// TODO: Move this out of the jpeg module.

// Based on T.871
// TODO: This is highly parallelizable (ideally do in CPU cache when decoding
// MCUs)
pub fn jpeg_ycbcr_to_rgb(inputs: &mut [u8]) {
    let clamp = |v: f32| -> u8 { v.round().max(0.0).min(255.0) as u8 };

    for tuple in inputs.chunks_mut(3) {
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

pub fn jpeg_rgb_to_ycbcr(inputs: &mut [u8]) {
    let clamp = |v: f32| -> u8 { v.round().max(0.0).min(255.0) as u8 };

    for tuple in inputs.chunks_mut(3) {
        let r = tuple[0] as f32;
        let g = tuple[1] as f32;
        let b = tuple[2] as f32;

        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb = -0.1687 * r - 0.3313 * g + 0.5 * b + 128.0;
        let cr = 0.5 * r - 0.4187 * g - 0.0813 * b + 128.0;

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
