use core::fmt::{Debug, Display};
use core::ops::{Add, Sub};
use core::str::FromStr;

use base_error::*;

// NOTE: We must separately verify that there is at least one digit in the
// number
regexp!(DECIMAL => "^([+-]?)([0-9]*)\\.?([0-9]*)");

/// Holds up to 9 digits of integer and 9 digits of fraction data.  
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    value: i64,
}

impl Decimal {
    /// Number of units needed to form a whole integer.
    ///
    /// This is chosen to give around half of the i64 bits to the fraction part
    /// of the number representation.
    ///
    /// 2^31 = 2147483648
    ///        1000000000 <- this value
    const UNITS_PER_INTEGER: i64 = 1000000000;

    fn is_nonnegative(&self) -> bool {
        self.value >= 0
    }

    /// Parses a decimal in human readable ASCII form from some bytes.
    ///
    /// Returns the parsed decimal and any bytes remaining after parsing the
    /// decimal.
    pub fn parse<'a>(data: &'a [u8]) -> Option<(Self, &'a [u8])> {
        let m = match DECIMAL.exec(data) {
            Some(v) => v,
            None => return None,
        };

        let sign = match m.group(1).unwrap() {
            b"-" => -1,
            b"+" | _ => 1,
        };

        let mut num_digits = 0;

        let mut integer = {
            let v = m.group_str(2).unwrap().unwrap();
            num_digits += v.len();

            if v.len() > 0 {
                match v.parse() {
                    Ok(v) => v,
                    Err(_) => return None,
                }
            } else {
                0
            }
        };

        let mut fraction = {
            let v = m.group(3).unwrap();
            num_digits += v.len();

            match Decimal::parse_fraction(v) {
                Some(v) => v,
                None => return None,
            }
        };

        if num_digits == 0 {
            return None;
        }

        integer *= sign;
        fraction *= sign;

        let mut v = Self {
            value: integer * Self::UNITS_PER_INTEGER + fraction,
        };

        Some((v, &data[m.last_index()..]))
    }

    fn parse_fraction(digits: &[u8]) -> Option<i64> {
        let mut value = 0;

        let mut scale = Self::UNITS_PER_INTEGER;

        for digit in digits.iter().cloned() {
            scale /= 10;
            if scale == 0 {
                return None;
            }

            let v = {
                if digit < b'0' || digit > b'9' {
                    return None;
                }

                (digit - b'0') as i64
            };

            value += v * scale;
        }

        Some(value)
    }

    fn stringify_fraction(&self, out: &mut String) {
        let mut frac = self.value.abs() % Self::UNITS_PER_INTEGER;

        if frac != 0 {
            out.push('.');
        }

        let mut scale = Self::UNITS_PER_INTEGER;
        while frac > 0 {
            scale /= 10;

            let digit = (frac / scale);
            out.push((b'0' + (digit as u8)) as char);
            frac -= scale * digit;
        }
    }
}

impl From<i32> for Decimal {
    fn from(value: i32) -> Self {
        Self {
            value: (value as i64) * Self::UNITS_PER_INTEGER,
        }
    }
}

impl From<f32> for Decimal {
    fn from(value: f32) -> Self {
        let integer = (value as i64) * Self::UNITS_PER_INTEGER;
        let fraction = (value.fract() * (Self::UNITS_PER_INTEGER as f32)) as i64;
        Self {
            value: integer + fraction,
        }
    }
}

impl Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let int = self.value / Self::UNITS_PER_INTEGER;

        f.pad_integral(self.is_nonnegative(), "", &int.abs().to_string())?;

        let mut frac = String::new();
        self.stringify_fraction(&mut frac);
        write!(f, "{}", frac)
    }
}

impl Debug for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl FromStr for Decimal {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Self::parse(s.as_bytes()) {
            Some((v, b"")) => Ok(v),
            _ => Err(err_msg("Invalid decimal")),
        }
    }
}

impl Add for Decimal {
    type Output = Decimal;

    fn add(mut self, rhs: Self) -> Self::Output {
        self.value += rhs.value;
        self
    }
}

impl Sub for Decimal {
    type Output = Self;

    fn sub(mut self, mut rhs: Self) -> Self::Output {
        self.value -= rhs.value;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_works() {
        let (v, rest) = Decimal::parse(b"1.5").unwrap();
        // assert_eq!(v.integer, 1);
        // assert_eq!(v.fraction, 500000000);
        assert_eq!(rest, b"");

        assert_eq!(v.to_string(), "1.5");

        assert_eq!((v + v).to_string(), "3");
        assert_eq!((v - v).to_string(), "0");

        assert_eq!(format!("{:02}", v), "01.5");

        assert_eq!(Decimal::from(-1.5f32).to_string(), "-1.5");

        assert_eq!(
            (Decimal::from(-1.5f32) + Decimal::from(-0.5)).to_string(),
            "-2"
        );

        assert_eq!(
            (Decimal::from(-1.5f32) + Decimal::from(-0.6)).to_string(),
            "-2.1"
        );
    }

    #[test]
    fn negative_point_five() {
        let (v, rest) = Decimal::parse(b"-0.5").unwrap();
        // assert_eq!(v.integer, 0);
        // assert_eq!(v.fraction, -500000000);
        assert_eq!(rest, b"");

        assert_eq!(format!("{}", v), "-0.5");
    }

    #[test]
    fn adding() {
        let a = Decimal::from(-0.7234f32);
        let b = Decimal::from(22.0f32);

        assert_eq!((a + b).to_string(), "21.2766");
    }
}
