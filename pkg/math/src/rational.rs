use core::cmp::Ordering;
use core::convert::From;
use core::fmt::{Debug, Display};
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::gcd::gcd;
use crate::matrix::element::ErrorEpsilon;
use crate::number::{AbsoluteValue, Cast, Number, One, Round, Zero};

/// Any number represented as a fraction of two integers.
///
/// Internally it is always stored as follows:
/// - Sign stored in the upper (numerator) of the fraction.
/// - The GCD of the numerator and denominitor is 1.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    upper: i64,
    lower: i64,
}

impl Default for Rational {
    fn default() -> Self {
        Self { upper: 0, lower: 1 }
    }
}

impl Zero for Rational {
    fn is_zero(&self) -> bool {
        self.upper == 0
    }

    fn zero() -> Self {
        Self { upper: 0, lower: 1 }
    }
}

impl One for Rational {
    fn is_one(&self) -> bool {
        self.upper == 1 && self.lower == 1
    }

    fn one() -> Self {
        Self { upper: 1, lower: 1 }
    }
}

impl ErrorEpsilon for Rational {
    fn error_epsilon() -> Self {
        Self::zero()
    }
}

impl AbsoluteValue for Rational {
    fn abs(self) -> Self {
        Self {
            upper: self.upper.abs(),
            lower: self.lower,
        }
    }
}

impl Neg for Rational {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            upper: -self.upper,
            lower: self.lower,
        }
    }
}

impl Number for Rational {}

impl Display for Rational {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.lower != 1 {
            write!(f, "{}/{}", self.upper, self.lower)
        } else {
            write!(f, "{}", self.upper)
        }
    }
}

impl Debug for Rational {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
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

        let x = gcd(upper.abs(), lower);
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

        fn exact_div(a: i64, b: i64) -> i64 {
            assert_eq!(a % b, 0);
            a / b
        }

        // Least common multiple
        let lower_gcd = gcd(self.lower, other.lower);
        let lcm = self.lower * exact_div(other.lower, lower_gcd);

        (
            self.upper * exact_div(lcm, self.lower),
            other.upper * exact_div(lcm, other.lower),
            lcm,
        )
    }

    fn common_lower_i128(self, other: Self) -> (i128, i128) {
        if self.lower == other.lower {
            return (self.upper as i128, other.upper as i128);
        }

        let a = (self.upper as i128) * (other.lower as i128);
        let b = (other.upper as i128) * (self.lower as i128);
        (a, b)
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

impl Round for Rational {
    fn round(self) -> Self {
        let mut v = self.clone();

        let down = v.upper.abs() % v.lower;
        let up = v.lower - down;
        if down < up {
            v.upper -= down * v.upper.signum();
        } else {
            v.upper += up * v.upper.signum();
        }

        v
    }
}

impl From<i16> for Rational {
    fn from(v: i16) -> Self {
        Self {
            upper: v as i64,
            lower: 1,
        }
    }
}

impl From<i32> for Rational {
    fn from(v: i32) -> Self {
        Self {
            upper: v as i64,
            lower: 1,
        }
    }
}

impl From<i64> for Rational {
    fn from(v: i64) -> Self {
        Self { upper: v, lower: 1 }
    }
}

impl Cast<Rational> for i64 {
    fn cast(self) -> Rational {
        self.into()
    }
}

impl Cast<i64> for Rational {
    fn cast(self) -> i64 {
        self.upper / self.lower
    }
}

impl Add for Rational {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let (upper1, upper2, lower) = self.common_lower(rhs);
        Self::new(upper1 + upper2, lower)
    }
}

impl AddAssign for Rational {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Rational {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let (upper1, upper2, lower) = self.common_lower(rhs);
        Self::new(upper1 - upper2, lower)
    }
}

impl SubAssign for Rational {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul for Rational {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(self.upper * rhs.upper, self.lower * rhs.lower)
    }
}

impl MulAssign for Rational {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Div for Rational {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self::new(self.upper * rhs.lower, self.lower * rhs.upper)
    }
}

impl DivAssign for Rational {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        // let (upper1, upper2, _) = self.common_lower(*other);
        let (upper1, upper2) = self.common_lower_i128(*other);
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

    #[test]
    fn negative_numbers() {
        let a = Rational::new(-3, 4);
        assert_eq!(a.upper, -3);
        assert_eq!(a.lower, 4);

        let a = Rational::new(-3, -4);
        assert_eq!(a.upper, 3);
        assert_eq!(a.lower, 4);

        let a = Rational::new(3, -4);
        assert_eq!(a.upper, -3);
        assert_eq!(a.lower, 4);

        let a = Rational::new(-4, 2);
        assert_eq!(a.upper, -2);
        assert_eq!(a.lower, 1);

        let a = Rational::new(-24, 30); // gcd=6
        assert_eq!(a.upper, -4);
        assert_eq!(a.lower, 5);

        let a = Rational::new(24, -30); // gcd=6
        assert_eq!(a.upper, -4);
        assert_eq!(a.lower, 5);
    }
}
