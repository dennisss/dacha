#![feature(
    core_intrinsics,
    const_constructor,
    proc_macro_hygiene,
    trait_alias,
    exclusive_range_pattern,
    wrapping_int_impl,
    asm
)]
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate asn;
#[macro_use]
extern crate lazy_static;
extern crate generic_array;
extern crate pkix;
extern crate typenum;

// TODO: Implement mlock utility from preventing swapping.

// TODO: Also implement < and > so that we can use this to implement
// datastructures storing secret keys.
/// Constant time comparison function between two byte arrays.
/// This is guranteed to always take the exact same amount of time
/// for two slices of the same byte length.
///
/// Returns whether or not the two slices are byte-wise equal.
#[no_mangle]
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    // TODO: Possibly check if they both point to the same location.
    // TODO: We must ensure that '&' is not optimized into a branching operation.

    let mut same: bool = true;

    const CMP_SIZE: usize = std::mem::size_of::<usize>();
    let n = a.len() / CMP_SIZE;

    // Compare full integers at a time.
    let mut i = 0;
    let last = CMP_SIZE * n;
    while i < last {
        let ai = usize::from_le_bytes(*array_ref![a, i, CMP_SIZE]);
        let bi = usize::from_le_bytes(*array_ref![b, i, CMP_SIZE]);
        i += CMP_SIZE;

        same = same && (ai == bi);
    }

    // Compare remaining bytes.
    while i < a.len() {
        same = same && (a[i] == b[i]);
        i += 1;
    }

    same
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test::*;

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

    #[test]
    fn constant_eq_leak_test() {
        const SIZE: usize = 10000;

        let zero = vec![0u8; SIZE];
        let all_set = vec![0xff; SIZE];
        let every_other_set = {
            let mut v = vec![0x00; SIZE];
            for i in (0..SIZE).step_by(2) {
                v[i] = 1;
            }
            v
        };
        let first_set = {
            let mut v = vec![0x00; SIZE];
            v[0] = 2;
            v
        };
        let first_set2 = {
            let mut v = vec![0x00; SIZE];
            v[0] = 10;
            v
        };

        let last_set = {
            let mut v = vec![0x00; SIZE];
            v[SIZE - 20] = 22;

            v
        };
        let last_set2 = {
            let mut v = vec![0x00; SIZE];
            v[SIZE - 20] = 60;
            v
        };

        // let random1 = crate::random::clocked_rng().generate_bytes()

        let input_data = &[
            &zero,
            &all_set,
            &every_other_set,
            &first_set,
            &first_set2,
            &last_set,
            &last_set2,
        ];

        let mut test_cases: Vec<(&[u8], &[u8])> = vec![];
        for a in input_data {
            for b in input_data {
                test_cases.push((a, b));
            }
        }
        for a in input_data {
            for b in input_data {
                test_cases.push((a, b));
            }
        }

        TimingLeakTest::new(
            test_cases.iter(),
            |(a, b)| constant_eq(*a, *b),
            TimingLeakTestOptions {
                num_iterations: 10000,
            },
        )
        .run();
    }
}

pub mod aead;
pub mod aes;
mod aes_generic;
pub mod chacha20;
pub mod checksum;
pub mod cipher;
pub mod des;
pub mod dh;
pub mod elliptic;
pub mod gcm;
pub mod hasher;
pub mod hkdf;
pub mod hmac;
pub mod md;
pub mod md5;
pub mod nist;
pub mod pem;
pub mod prime;
pub mod random;
pub mod rsa;
pub mod sha1;
pub mod sha224;
pub mod sha256;
pub mod sha384;
pub mod sha512;
mod sha_test;
pub mod sip;
pub mod test;
pub mod tls;
pub mod utils;
pub mod x509;
