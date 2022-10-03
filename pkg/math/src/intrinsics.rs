#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
pub fn to_m128i(v: &[u8]) -> __m128i {
    assert_eq!(v.len(), 16);
    unsafe { _mm_loadu_si128(core::mem::transmute(v.as_ptr())) }
}

#[cfg(target_arch = "x86_64")]
pub fn u64_to_m128i(v: u64) -> __m128i {
    unsafe { _mm_set_epi64x(0, v as i64) }
}

#[cfg(target_arch = "x86_64")]
pub fn from_m128i(v: __m128i, out: &mut [u8]) {
    assert_eq!(out.len(), 16);
    unsafe {
        _mm_storeu_si128(core::mem::transmute(out.as_mut_ptr()), v);
    }
}

#[cfg(target_arch = "x86_64")]
pub fn u64_from_m128i(v: __m128i) -> u64 {
    let mut out: u64 = 0;
    unsafe { _mm_storel_epi64(core::mem::transmute(&mut out), v) }
    out
}
