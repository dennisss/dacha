// https://en.wikipedia.org/wiki/Extended_Euclidean_algorithm#Pseudocode
fn extended_gcd(a: isize, b: isize) -> isize {
    let mut s = 0;
    let mut old_s = 1;
    let mut t = 1;
    let mut old_t = 0;
    let mut r = b;
    let mut old_r = a;

    while r != 0 {
        let quotient = old_r / r;

        let tmp_r = r;
        r = old_r - quotient * r;
        old_r = tmp_r;

        let tmp_s = s;
        s = old_s - quotient * s;
        old_s = tmp_s;

        let tmp_t = t;
        t = old_t - quotient * t;
        old_t = tmp_t;
    }

    // println!("Bezout coefficients: {} {}", old_s, old_t);
    // println!("greatest common divisor: {}", old_r);
    // println!("quotients by the gcd: {} {}", t, s);
    old_r
}

/// Computes the greatest common divisor of two integers using Euclid's
/// algorithm
pub fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        tup!((a, b) = (b, a % b));
    }

    a
}
