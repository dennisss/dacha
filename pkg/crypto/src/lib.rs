#![feature(
    core_intrinsics,
    const_constructor,
    proc_macro_hygiene,
    trait_alias,
    exclusive_range_pattern,
    wrapping_int_impl,
    asm,
    let_chains,
    drain_filter
)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate common;
#[cfg(feature = "std")]
#[macro_use]
extern crate macros;
#[cfg(feature = "std")]
#[macro_use]
extern crate asn;
#[cfg(feature = "std")]
extern crate generic_array;
#[cfg(feature = "std")]
extern crate pkix;
extern crate typenum;
#[cfg(feature = "std")]
#[macro_use]
extern crate file;

// TODO: Implement mlock utility from preventing swapping.

// TODO: Also implement < and > so that we can use this to implement
// datastructures storing secret keys.
/// Constant time comparison function between two byte arrays.
/// This is guranteed to always take the exact same amount of time
/// for two slices of the same byte length.
///
/// Returns whether or not the two slices are byte-wise equal.
#[no_mangle]
#[inline(never)]
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    // Will become non-zero if we detect non-matching bytes.
    let mut diff: usize = 0;

    const CMP_SIZE: usize = core::mem::size_of::<usize>();
    let n = a.len() / CMP_SIZE;

    // Compare full integers at a time.
    let mut i = 0;
    let last = CMP_SIZE * n;
    while i < last {
        let ai = usize::from_ne_bytes(*array_ref![a, i, CMP_SIZE]);
        let bi = usize::from_ne_bytes(*array_ref![b, i, CMP_SIZE]);
        i += CMP_SIZE;

        diff |= ai ^ bi;
    }

    // Compare remaining bytes.
    while i < a.len() {
        diff |= (a[i] ^ b[i]) as usize;
        i += 1;
    }

    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test::*;

    use std::println;
    use std::vec;
    use std::vec::Vec;

    #[test]
    fn constant_eq_test() {
        assert!(!constant_eq(&[1, 2], &[1, 2, 3]));
        assert!(!constant_eq(&[1, 2, 3], &[1, 2]));
        assert!(constant_eq(&[], &[]));
        assert!(constant_eq(&[0, 0], &[0, 0]));
        assert!(constant_eq(&[10, 20], &[10, 20]));

        assert!(constant_eq(
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        ));
        assert!(!constant_eq(
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            &[1, 2, 3, 4, 5, 6, 7, 8, 11, 12]
        ));
    }

    #[test]
    fn constant_eq_timing_test() {
        // TODO: Should also test that it actually works as a comparer (not just timing)

        let niters = 10000;

        {
            let mut buf = [42u8; 4096];
            buf[2] = 17;

            let mut buf2 = [42u8; 4096];
            buf2[2] = 99;

            {
                let start = std::time::Instant::now();
                for i in 0..niters {
                    assert!(!constant_eq(&buf, &buf2));
                }
                let end = std::time::Instant::now();

                println!("Best case constant_eq: {:?}", end.duration_since(start));
            }

            {
                let start = std::time::Instant::now();
                for i in 0..niters {
                    assert!((&buf as &[u8]) != (&buf2 as &[u8]));
                }
                let end = std::time::Instant::now();

                println!("Best case cheap compare: {:?}", end.duration_since(start));
            }
        }

        {
            let mut buf = [42u8; 4096];
            buf[4080] = 17;

            let mut buf2 = [42u8; 4096];
            buf2[4080] = 99;

            {
                let start = std::time::Instant::now();
                for i in 0..niters {
                    assert!(!constant_eq(&buf, &buf2));
                }
                let end = std::time::Instant::now();

                println!("Worst case constant_eq: {:?}", end.duration_since(start));
            }

            {
                let start = std::time::Instant::now();
                for i in 0..niters {
                    assert!((&buf as &[u8]) != (&buf2 as &[u8]));
                }
                let end = std::time::Instant::now();

                println!("Worst case cheap compare: {:?}", end.duration_since(start));
            }
        }
    }
}

#[cfg(feature = "std")]
pub mod aead;
#[cfg(feature = "std")]
pub mod aes;
#[cfg(feature = "std")]
mod aes_generic;
pub mod ccm;
#[cfg(feature = "std")]
pub mod chacha20;
pub mod checksum;
pub mod cipher;
#[cfg(feature = "std")]
pub mod des;
#[cfg(feature = "std")]
pub mod dh;
#[cfg(feature = "std")]
pub mod elliptic;
#[cfg(feature = "std")]
pub mod gcm;
pub mod hasher;
#[cfg(feature = "std")]
pub mod hkdf;
#[cfg(feature = "std")]
pub mod hmac;
#[cfg(feature = "std")]
pub mod md;
#[cfg(feature = "std")]
pub mod md5;
#[cfg(feature = "std")]
pub mod nist;
#[cfg(feature = "std")]
pub mod pem;
#[cfg(feature = "std")]
pub mod prime;
#[cfg(feature = "std")]
pub mod random;
#[cfg(feature = "std")]
pub mod rsa;
#[cfg(feature = "std")]
pub mod sha1;
#[cfg(feature = "std")]
pub mod sha224;
#[cfg(feature = "std")]
pub mod sha256;
#[cfg(feature = "std")]
pub mod sha384;
#[cfg(feature = "std")]
pub mod sha512;
#[cfg(feature = "std")]
mod sha_test;
pub mod sip;
#[cfg(feature = "std")]
pub mod tls;
pub mod utils;
#[cfg(feature = "std")]
pub mod x509;
