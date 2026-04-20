use core::fmt::Debug;
use std::time::Duration;
use std::convert::From;
use std::ops::Sub;

#[derive(Clone, Copy, PartialEq)]
pub struct SignedDuration {
    pub sign: i32,
    pub duration: Duration
}

impl Debug for SignedDuration {
    fn fmt(
        &self,
        f: &mut ::core::fmt::Formatter<'_>,
    ) -> ::core::result::Result<(), ::core::fmt::Error> {
        let sign = if self.sign >= 0 { "+" } else { "-" };
        write!(f, "{}{:?}", sign, self.duration)
    }
}

impl SignedDuration {
    pub fn new(sign: i32, duration: Duration) -> Self {
        Self { sign, duration }
    }

    pub fn from_micros(v: i64) -> Self {
        let sign = if v > 0 { 1 } else { -1 };
        let duration = Duration::from_micros(v.abs() as u64);
        Self { sign, duration }
    }

    fn as_i128(&self) -> i128 {
        (self.duration.as_nanos() as i128) * (self.sign as i128)
    }

    fn from_i128(v: i128) -> Self {
        let sign = v.signum() as i32;
        Self {
            sign,
            duration: Duration::from_nanos_u128(v.abs() as u128)
        }
    }

    pub fn as_secs_f64(&self) -> f64 {
        self.duration.as_secs_f64() * (self.sign as f64)
    }
}

impl From<Duration> for SignedDuration {
    fn from(duration: Duration) -> Self {
        Self {
            sign: 1,
            duration
        }
    }
}

impl Sub for SignedDuration {
    type Output = Self;

    fn sub(mut self, mut rhs: Self) -> Self::Output {
        Self::from_i128(self.as_i128() - rhs.as_i128())
    }
}