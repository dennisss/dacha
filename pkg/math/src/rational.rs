use core::cmp::Ordering;
use core::convert::From;
use core::ops::{Add, Div, Mul, Sub};

use crate::gcd::gcd;

/// Any number represented as a fraction of two integers.
///
/// Internally it is always stored as follows:
/// - Sign stored in the upper (numerator) of the fraction.
/// - The GCD of the numerator and denominitor is 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rational {
    upper: i64,
    lower: i64,
}

impl Rational {
    fn new(mut upper: i64, mut lower: i64) -> Self {
        assert!(lower != 0);
        if upper == 0 {
            return Self { upper: 0, lower: 1 };
        }

        if lower < 0 {
            upper *= -1;
            lower *= -1;
        }

        let x = gcd(upper, lower);
        Self {
            upper: upper / x,
            lower: lower / x,
        }
    }

    /// Returns (upper1, upper2, lower)
    fn common_lower(self, other: Self) -> (i64, i64, i64) {
        if self.lower == other.lower {
            return (self.upper, other.upper, self.lower);
        }

        (
            self.upper * other.lower,
            other.upper * self.lower,
            self.lower * other.lower,
        )
    }

    pub fn abs(self) -> Self {
        Self {
            upper: self.upper.abs(),
            lower: self.lower.abs(),
        }
    }

    pub fn signum(self) -> Self {
        Self {
            upper: self.upper.signum(),
            lower: 1,
        }
    }

    pub fn to_f32(self) -> f32 {
        (self.upper as f32) / (self.lower as f32)
    }
}

impl From<i64> for Rational {
    fn from(v: i64) -> Self {
        Self { upper: v, lower: 1 }
    }
}

impl Add for Rational {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let (upper1, upper2, lower) = self.common_lower(rhs);
        Self::new(upper1 + upper2, lower)
    }
}

impl Sub for Rational {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let (upper1, upper2, lower) = self.common_lower(rhs);
        Self::new(upper1 - upper2, lower)
    }
}

impl Mul for Rational {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(self.upper * rhs.upper, self.lower * rhs.lower)
    }
}

impl Div for Rational {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self::new(self.upper * rhs.lower, self.lower * rhs.upper)
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        let (upper1, upper2, _) = self.common_lower(*other);
        upper1.cmp(&upper2)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        let a = Rational::from(2);
        let b = Rational::from(4);

        let c = a / b; // 1/2
        assert_eq!(c.upper, 1);
        assert_eq!(c.lower, 2);
        assert_eq!(c, c);

        let d = b * c;
        assert_eq!(d.upper, 2);
        assert_eq!(d.lower, 1);

        assert_eq!(d, a);

        let e = Rational::new(1, 2) + Rational::new(3, 5);
        assert_eq!(e.upper, 11);
        assert_eq!(e.lower, 10);
    }
}
