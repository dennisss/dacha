#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

pub const SIMD_THRESHOLDING_CHUNK_SIZE: usize = 16;

/// Checks if any element in the 16-byte buffer is strictly greater than the threshold.
pub fn is_any_greater_than_threshold(buf: &[u8; SIMD_THRESHOLDING_CHUNK_SIZE], threshold: u8) -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            // Load the 16 bytes into a NEON register (uint8x16_t)
            let vector_data = vld1q_u8(buf.as_ptr());

            // Compute the maximum value across the entire vector.
            let max_val = vmaxvq_u8(vector_data);

            max_val > threshold
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        buf.iter().any(|&val| val > threshold)
    }
}
