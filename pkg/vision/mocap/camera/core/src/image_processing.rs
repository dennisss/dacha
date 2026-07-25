// These are image processing utilities that are mainly used in the MJPEGEncoder

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// Downscales an 8-bit grayscale image by 2x in both dimensions.
/// `width` and `height` must be even numbers.
/// 
/// This function automatically detects and dispatches to the most
/// efficient SIMD implementation available for the target architecture.
pub fn downscale_2x(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    assert!(width % 2 == 0, "Width must be even");
    assert!(height % 2 == 0, "Height must be even");
    assert!(src.len() == width * height, "Source buffer wrong size");
    assert!(dst.len() == (width / 2) * (height / 2), "Destination buffer wrong size");

    #[cfg(target_arch = "aarch64")]
    {
        // ARM processors almost universally support NEON.
        unsafe {
            downscale_2x_neon(src, dst, width, height);
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        // x86_64 requires runtime feature detection for AVX2.
        if is_x86_feature_detected!("avx2") {
            unsafe {
                downscale_2x_avx2(src, dst, width, height);
            }
        } else {
            // Fallback to scalar if running on a very old CPU.
            downscale_2x_scalar(src, dst, width, height);
        }
    }

    // Fallback for completely unsupported architectures (e.g., WASM, RISC-V without V extension)
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        downscale_2x_scalar(src, dst, width, height);
    }
}

/// Applies a binary threshold to an 8-bit grayscale image.
/// Pixels <= threshold become 0, pixels > threshold become 255.
pub fn apply_threshold(src: &[u8], dst: &mut [u8], threshold: u8) {
    assert!(src.len() == dst.len(), "Destination buffer wrong size");

    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            apply_threshold_neon(src, dst, threshold);
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe {
                apply_threshold_avx2(src, dst, threshold);
            }
        } else {
            apply_threshold_scalar(src, dst, threshold);
        }
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        apply_threshold_scalar(src, dst, threshold);
    }
}

/// Standard scalar fallback implementation.
pub fn downscale_2x_scalar(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    let src_stride = width;
    let dst_stride = width / 2;

    for y in (0..height).step_by(2) {
        let src_row0_start = y * src_stride;
        let src_row1_start = (y + 1) * src_stride;
        let dst_row_start = (y / 2) * dst_stride;

        for x in (0..width).step_by(2) {
            // Using u16 to prevent overflow during addition
            let sum = src[src_row0_start + x] as u16
                + src[src_row0_start + x + 1] as u16
                + src[src_row1_start + x] as u16
                + src[src_row1_start + x + 1] as u16;
            
            // Add 2 for exact rounding before dividing by 4
            dst[dst_row_start + (x / 2)] = ((sum + 2) >> 2) as u8;
        }
    }
}

/// Scalar fallback for thresholding.
pub fn apply_threshold_scalar(src: &[u8], dst: &mut [u8], threshold: u8) {
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = if *s <= threshold { 0 } else { 255 };
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn downscale_2x_neon(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    let src_stride = width;
    let dst_stride = width / 2;

    for y in (0..height).step_by(2) {
        let src_row0 = src.as_ptr().add(y * src_stride);
        let src_row1 = src.as_ptr().add((y + 1) * src_stride);
        let dst_row = dst.as_mut_ptr().add((y / 2) * dst_stride);

        let mut x = 0;
        
        // Process 16 input pixels per iteration (yielding 8 output pixels)
        while x + 16 <= width {
            let r0 = vld1q_u8(src_row0.add(x));
            let r1 = vld1q_u8(src_row1.add(x));

            // Pairwise add horizontally, widening 8-bit to 16-bit.
            let sum0 = vpaddlq_u8(r0);
            let sum1 = vpaddlq_u8(r1);

            // Add the two rows together vertically
            let sum = vaddq_u16(sum0, sum1);

            // Shift right by 2 (divide by 4) with exact rounding, narrow to 8-bit
            let out = vrshrn_n_u16(sum, 2);

            // Store the 8 downscaled pixels
            vst1_u8(dst_row.add(x / 2), out);

            x += 16;
        }

        // Handle any remaining pixels
        while x < width {
            let sum = *src_row0.add(x) as u16
                + *src_row0.add(x + 1) as u16
                + *src_row1.add(x) as u16
                + *src_row1.add(x + 1) as u16;
            
            *dst_row.add(x / 2) = ((sum + 2) >> 2) as u8;
            x += 2;
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn apply_threshold_neon(src: &[u8], dst: &mut [u8], threshold: u8) {
    let len = src.len();
    let mut i = 0;

    // Broadcast the threshold to all 16 lanes of a NEON register
    let thresh_vec = vdupq_n_u8(threshold);

    // Process 16 pixels at a time
    while i + 16 <= len {
        // Load 16 pixels
        let pixels = vld1q_u8(src.as_ptr().add(i));

        // Compare: vcgtq_u8 returns 0xFF if pixels > thresh_vec, 0x00 otherwise.
        // This is exactly the 0 or 255 output we want.
        let result = vcgtq_u8(pixels, thresh_vec);

        // Store the result
        vst1q_u8(dst.as_mut_ptr().add(i), result);

        i += 16;
    }

    // Handle remaining pixels
    while i < len {
        *dst.get_unchecked_mut(i) = if *src.get_unchecked(i) <= threshold { 0 } else { 255 };
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn downscale_2x_avx2(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    let src_stride = width;
    let dst_stride = width / 2;

    // We use maddubs for horizontal addition. It multiplies u8 pairs by i8 pairs and adds them.
    // By multiplying by [1, 1, 1, 1...], it effectively just adds adjacent u8s into an i16.
    let ones = _mm256_set1_epi8(1);
    
    // To do exact rounding division by 4, we need to add 2 to the sum before shifting.
    let rounding_add = _mm256_set1_epi16(2);

    for y in (0..height).step_by(2) {
        let src_row0 = src.as_ptr().add(y * src_stride);
        let src_row1 = src.as_ptr().add((y + 1) * src_stride);
        let dst_row = dst.as_mut_ptr().add((y / 2) * dst_stride);

        let mut x = 0;
        
        // Process 32 input pixels per iteration (yielding 16 output pixels)
        while x + 32 <= width {
            // Load 32 bytes from top and bottom rows
            let r0 = _mm256_loadu_si256(src_row0.add(x) as *const __m256i);
            let r1 = _mm256_loadu_si256(src_row1.add(x) as *const __m256i);

            // Horizontally add adjacent 8-bit pixels into 16-bit integers
            // maddubs_epi16 does: (r0[0]*1 + r0[1]*1), (r0[2]*1 + r0[3]*1)...
            let sum0 = _mm256_maddubs_epi16(r0, ones);
            let sum1 = _mm256_maddubs_epi16(r1, ones);

            // Vertically add the two rows and apply the rounding factor (+2)
            let mut sum = _mm256_add_epi16(sum0, sum1);
            sum = _mm256_add_epi16(sum, rounding_add);

            // Divide by 4 (shift right by 2)
            sum = _mm256_srli_epi16(sum, 2);

            // Pack the 16-bit integers back down to 8-bit unsigned integers.
            // packus_epi16 saturates to u8. 
            // Note: packus_epi16 interleaves data across the 128-bit lanes. 
            // Instead of [0, 1, 2, 3... 15], it produces [0..7, 16..23, 8..15, 24..31].
            let packed = _mm256_packus_epi16(sum, sum); // Pack with itself just to satisfy args

            // We must permute to fix the lane interleaving caused by packus in AVX2.
            // 0xD8 = 11_01_10_00 binary, which maps chunks [0, 2, 1, 3] to [0, 1, 2, 3]
            let permuted = _mm256_permute4x64_epi64(packed, 0xD8);

            // Store the lower 128 bits (16 bytes) containing our downscaled pixels
            // Cast the 256-bit vector to a 128-bit vector to extract the low half.
            let out_128 = _mm256_castsi256_si128(permuted);
            _mm_storeu_si128(dst_row.add(x / 2) as *mut __m128i, out_128);

            x += 32;
        }

        // Handle any remaining pixels (scalar fallback for the tail)
        while x < width {
            let sum = *src_row0.add(x) as u16
                + *src_row0.add(x + 1) as u16
                + *src_row1.add(x) as u16
                + *src_row1.add(x + 1) as u16;
            
            *dst_row.add(x / 2) = ((sum + 2) >> 2) as u8;
            x += 2;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn apply_threshold_avx2(src: &[u8], dst: &mut [u8], threshold: u8) {
    let len = src.len();
    let mut i = 0;

    // x86 doesn't have a direct unsigned 8-bit greater-than comparison in AVX2.
    // However, it does have _mm256_cmpgt_epi8 (signed comparison) or we can use
    // _mm256_subs_epu8 (unsigned subtraction with saturation) followed by checking for > 0.
    // Wait, AVX2 lacks a direct unsigned byte comparison. But wait, `pixels > threshold`
    // is equivalent to `(pixels - threshold) > 0` which requires unsigned math, but we only have signed compare.
    // The trick for unsigned comparison `a > b` using signed comparison is to subtract 128 from both `a` and `b`,
    // casting them to signed bytes.
    // However, an even simpler trick for `a > b` in unsigned space:
    // If we do unsigned saturated subtraction `_mm256_subs_epu8(pixels, threshold)`,
    // any pixel <= threshold becomes 0. Any pixel > threshold becomes > 0.
    // We can then compare the result to 0 to get 0xFF for those > 0.
    
    let thresh_vec = _mm256_set1_epi8(threshold as i8);
    let zero = _mm256_setzero_si256();

    while i + 32 <= len {
        let pixels = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);

        // pixels - threshold (saturating unsigned)
        // If pixel <= threshold, sub = 0.
        // If pixel > threshold, sub > 0.
        let sub = _mm256_subs_epu8(pixels, thresh_vec);

        // Compare sub > 0 (signed compare is fine here because sub is strictly positive or zero, max 255 which becomes negative if > 127... wait)
        // Ah, if `sub` is 128-255, it appears as negative in signed comparison `cmpgt`, so `sub > 0` fails!
        // So we instead use _mm256_cmpeq_epi8(sub, zero) which returns 0xFF for pixels <= threshold.
        // Then we invert the result using _mm256_andnot_si256 or simply bitwise NOT!
        let is_less_or_equal = _mm256_cmpeq_epi8(sub, zero);
        
        // Invert: ~is_less_or_equal
        // We want 0xFF for > threshold, and 0x00 for <= threshold.
        // If is_less_or_equal is 0xFF, ~0xFF = 0x00.
        // If is_less_or_equal is 0x00, ~0x00 = 0xFF.
        // AVX2 doesn't have a single-instruction bitwise NOT, but we can XOR with all ones.
        // But actually, _mm256_andnot_si256(is_less_or_equal, all_ones) works.
        let all_ones = _mm256_set1_epi8(-1);
        let result = _mm256_andnot_si256(is_less_or_equal, all_ones);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, result);

        i += 32;
    }

    // Handle tail
    while i < len {
        *dst.get_unchecked_mut(i) = if *src.get_unchecked(i) <= threshold { 0 } else { 255 };
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downscale() {
        // Simple 4x4 image
        // 10 20 | 30 40
        // 30 40 | 50 60
        // --------------
        // 10 10 | 10 10
        // 10 10 | 10 10
        let src = vec![
            10, 20, 30, 40,
            30, 40, 50, 60,
            10, 10, 10, 10,
            10, 10, 10, 10,
        ];
        
        let mut dst = vec![0u8; 4]; // 2x2 output
        
        downscale_2x(&src, &mut dst, 4, 4);
        
        // Expected averages:
        // Top left: (10+20+30+40)/4 = 100/4 = 25
        // Top right: (30+40+50+60)/4 = 180/4 = 45
        // Bottom left: (10+10+10+10)/4 = 40/4 = 10
        // Bottom right: (10+10+10+10)/4 = 40/4 = 10
        assert_eq!(dst, vec![25, 45, 10, 10]);
    }

    #[test]
    fn test_threshold() {
        let src = vec![0, 50, 100, 127, 128, 200, 255];
        let mut dst = vec![0; 7];
        
        apply_threshold(&src, &mut dst, 127);
        
        // Expected: <= 127 becomes 0, > 127 becomes 255
        assert_eq!(dst, vec![0, 0, 0, 0, 255, 255, 255]);
    }
}