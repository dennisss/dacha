use std::fmt::{Debug, Write};
use std::{convert::TryFrom, str::FromStr};

use common::errors::*;

regexp!(DURATION_PATTERN => "^(-)?P(?:([0-9]+)Y)?(?:([0-9]+)M)?(?:([0-9]+)D)?(?:T(?:([0-9]+)H)?(?:([0-9]+)M)?(?:([0-9]+)(?:\\.([0-9]+))?S)?)?$");

const NANOS_PER_SECOND: u64 = 1000000000;

/// Duration of the form 'PnYnMnDTnHnMnS'
///
/// See https://www.w3schools.com/xml/schema_dtypes_date.asp
#[derive(Debug, Clone)]
pub struct Duration {
    pub negative: bool,
    pub years: u64,
    pub months: u64,
    pub days: u64,
    pub hours: u64,
    pub minutes: u64,
    pub seconds: u64,
    pub nanos: u64,
}

impl FromStr for Duration {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let m = DURATION_PATTERN
            .exec(s)
            .ok_or_else(|| err_msg("Duration has invalid format"))?;

        let mut have_some_number = false;

        let get_number = |idx: usize, have_some_number: &mut bool| -> Result<u64> {
            if let Some(s) = m.group_str(idx) {
                *have_some_number = true;
                Ok(s?.parse()?)
            } else {
                Ok(0)
            }
        };

        let negative = m.group(1).is_some();
        let years = get_number(2, &mut have_some_number)?;
        let months = get_number(3, &mut have_some_number)?;
        let days = get_number(4, &mut have_some_number)?;
        let hours = get_number(5, &mut have_some_number)?;
        let minutes = get_number(6, &mut have_some_number)?;
        let seconds = get_number(7, &mut have_some_number)?;

        let nanos = {
            if let Some(s) = m.group_str(8) {
                let s = s?;
                if s.len() > 9 {
                    return Err(err_msg(
                        "Sub-second duration more has more than nanosecond precision",
                    ));
                }

                // Right pad zeros.
                let mut s = s.to_string();
                while s.len() < 9 {
                    s.push('0');
                }

                s.parse()?
            } else {
                0
            }
        };

        Ok(Self {
            negative,
            years,
            months,
            days,
            hours,
            minutes,
            seconds,
            nanos,
        })
    }
}

// impl Debug for Duration {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.to_string())
//     }
// }

impl ToString for Duration {
    fn to_string(&self) -> String {
        let mut out = String::new();
        if self.negative {
            out.push('-');
        }
        out.push('P');
        let empty_len = out.len();

        if self.years > 0 {
            write!(&mut out, "{}Y", self.years).unwrap();
        }
        if self.months > 0 {
            write!(&mut out, "{}M", self.months).unwrap();
        }
        if self.days > 0 {
            write!(&mut out, "{}D", self.days).unwrap();
        }
        if self.hours > 0 || self.minutes > 0 || self.seconds > 0 || self.nanos > 0 {
            out.push('T');

            if self.hours > 0 {
                write!(&mut out, "{}H", self.hours).unwrap();
            }
            if self.minutes > 0 {
                write!(&mut out, "{}M", self.minutes).unwrap();
            }

            if self.seconds > 0 || self.nanos > 0 {
                write!(&mut out, "{}", self.seconds).unwrap();

                if self.nanos > 0 {
                    let s = format!(".{:09}", self.nanos);
                    write!(&mut out, "{}", s.trim_end_matches('0')).unwrap();
                }

                out.push('S');
            }
        }

        if out.len() == empty_len {
            out.push_str("T0S");
        }

        out
    }
}

impl Duration {
    pub fn to_std_duration(&self) -> Option<std::time::Duration> {
        if self.negative || self.years > 0 || self.months > 0 || self.nanos > NANOS_PER_SECOND {
            return None;
        }

        let mut secs = self.seconds;
        secs += 60 * self.minutes;
        secs += 60 * 60 * self.hours;
        secs += 60 * 60 * 24 * self.days;

        Some(std::time::Duration::from_secs(secs) + std::time::Duration::from_nanos(self.nanos))
    }
}

impl<'data> reflection::ParseFromValue<'data> for Duration {
    fn parse_from_primitive(value: reflection::PrimitiveValue<'data>) -> Result<Self> {
        let s = match &value {
            reflection::PrimitiveValue::Str(s) => *s,
            reflection::PrimitiveValue::String(s) => s.as_str(),
            _ => return Err(err_msg("Expected a string to parse")),
        };

        s.parse()
    }
}

impl reflection::SerializeTo for Duration {
    fn serialize_to<Output: reflection::ValueSerializer>(&self, out: Output) -> Result<()> {
        out.serialize_primitive(reflection::PrimitiveValue::String(self.to_string()))
    }
}

/*
Valid:

PT60S
P0Y0M0DT0H3M30.000S


PT1004199059S
PT130S
PT2M10S
P1DT2S
-P1Y
P1Y2M3DT5H20M30.123S

Invalid


1Y (the leading P is missing)
P1S (the T separator is missing)
P-1Y (all parts must be positive)
P1M2Y (the parts order is significant and Y must precede M)
P1Y-1M (all parts must be positive)

*/
