// Combinatorics utilities.

pub fn factorial(mut x: usize) -> usize {
    let mut v = 1;
    while x > 1 {
        v *= x;
        x -= 1;
    }

    v
}

/// Computes 'n choose k'
pub fn bin_coeff(n: usize, k: usize) -> usize {
    factorial(n) / (factorial(k) * factorial(n - k))
}
