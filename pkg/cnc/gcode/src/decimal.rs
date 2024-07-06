use core::fmt::{Debug, Display};
use core::ops::{Add, Sub};
use core::str::FromStr;

use base_error::*;

// NOTE: We must separately verify that there is at least one digit in the
// number
//
// TODO: REquire a complete match ending with '$' here.
regexp!(DECIMAL => "^([+-]?)([0-9]*)\\.*([0-9]*)");

/// Holds up to 9 digits of integer and 9 digits of fraction data.  
///
/// The main goal of this struct is to exactly represent a gcode real value
/// (parsing a gcode number and then re-stringifying the Decimal should produce
/// equal values aside from minor formatting differences).
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

    pub fn parse_complete(data: &[u8]) -> Option<Self> {
        if let Some((v, b"")) = Self::parse(data) {
            Some(v)
        } else {
            None
        }
    }

    /// Parses a decimal in human readable ASCII form from some bytes.
    ///
    /// Returns the parsed decimal and any bytes remaining after parsing the
    /// decimal.
    pub fn parse<'a>(mut data: &'a [u8]) -> Option<(Self, &'a [u8])> {
        let mut sign = 1;
        let mut num_digits = 0;

        if data.len() > 0 {
            match data[0] {
                b'+' => {
                    sign = 1;
                    data = &data[1..];
                }
                b'-' => {
                    sign = -1;
                    data = &data[1..];
                }
                _ => {} // Retry later as a digit.
            }
        }

        // TODO: Limit the max size of this integer part to avoid overflow later.
        let mut integer = 0;
        while data.len() > 0 {
            let digit = data[0];
            if digit < b'0' || digit > b'9' {
                break;
            }

            // Overflow
            if num_digits == 9 {
                return None;
            }

            let v = (digit - b'0') as i64;
            integer = 10 * integer + v;
            num_digits += 1;
            data = &data[1..];
        }

        let mut num_dots = 0;
        while data.len() > 0 && data[0] == b'.' {
            num_dots += 1;
            data = &data[1..];
        }

        let mut fraction = 0;
        if num_dots > 0 {
            let mut overflow = false;

            fraction = match Self::parse_fraction(data, &mut num_digits, &mut overflow) {
                Some((v, r)) => {
                    data = r;
                    v
                }
                None => 0,
            };

            if overflow {
                return None;
            }
        }

        // Old slow version
        /*
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
        */

        if num_digits == 0 {
            return None;
        }

        integer *= sign;
        fraction *= sign;

        let mut v = Self {
            value: integer * Self::UNITS_PER_INTEGER + fraction,
        };

        Some((v, data))

        // Some((v, &data[m.last_index()..]))
    }

    fn parse_fraction<'a>(
        mut digits: &'a [u8],
        num_digits: &mut usize,
        overflow: &mut bool,
    ) -> Option<(i64, &'a [u8])> {
        let mut value = 0;

        let mut scale = Self::UNITS_PER_INTEGER;

        while digits.len() > 0 {
            let digit = digits[0];

            scale /= 10;
            if scale == 0 {
                *overflow = true;
                return None;
            }

            let v = {
                if digit < b'0' || digit > b'9' {
                    break;
                }

                (digit - b'0') as i64
            };

            value += v * scale;
            digits = &digits[1..];
            *num_digits += 1;
        }

        Some((value, digits))
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

    pub fn to_f32(&self) -> f32 {
        self.to_string().parse::<f32>().unwrap()
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

    // TODO: Test the largest possible parseable numbers and verify verify that
    // parsing can't crash if we overflow.s

    // 000000000

    #[test]
    fn decimal_works() {
        let (v, rest) = Decimal::parse(b"1.5").unwrap();
        assert_eq!(v.value, 1500000000);
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
        assert_eq!(v.value, -500000000);
        assert_eq!(rest, b"");
        assert_eq!(format!("{}", v), "-0.5");
    }

    #[test]
    fn parsing_overflow() {
        assert_eq!(
            Decimal::parse(b"0.999999999"),
            Some((Decimal { value: 999999999 }, &b""[..]))
        );
        assert_eq!(Decimal::parse(b"0.9999999999"), None);

        assert_eq!(
            Decimal::parse(b"999999999"),
            Some((
                Decimal {
                    value: 999999999000000000
                },
                &b""[..]
            ))
        );

        assert_eq!(Decimal::parse(b"9999999999"), None);
    }

    #[test]
    fn parsing_invalid() {
        assert_eq!(Decimal::parse(b""), None);
        assert_eq!(Decimal::parse(b"+"), None);
        assert_eq!(Decimal::parse(b"-"), None);
        assert_eq!(Decimal::parse(b"-."), None);
        assert_eq!(Decimal::parse(b"."), None);
        assert_eq!(Decimal::parse(b"..."), None);
    }

    #[test]
    fn adding() {
        let a = Decimal::from(-0.7234f32);
        let b = Decimal::from(22.0f32);

        assert_eq!((a + b).to_string(), "21.2766");
    }
}
