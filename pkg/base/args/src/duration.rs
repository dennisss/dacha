use std::time::Duration;
use alloc::string::String;

use base_error::*;

use crate::{ArgType, RawArgValue};

impl ArgType for Duration {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        let s = match raw_arg {
            RawArgValue::Bool(_) => return Err(err_msg("Expected string, got bool")),
            RawArgValue::String(s) => s,
        };

        let mut num = String::new();

        let mut total = 0;

        for c in s.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                continue;
            } else if c == ' ' {
                continue;
            }

            let unit = match c {
                's' => 1,
                'm' => 60,
                'h' => 60*60,
                'd' => 60*60*24,
                'w' => 60*60*24*7,
                _ => return Err(err_msg("Unknown unit"))
            };

            total += num.parse::<u64>()? * unit;
            num.clear();
        }
        
        // Rest is just seconds.
        if !num.is_empty() {
            total += num.parse::<u64>()?;
        }

        Ok(Duration::from_secs(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::ToString;

    #[test]
    fn works() {

        let tests: &'static [(&'static str, u64)] = &[
            ("1", 1),
            ("543", 543),
            ("10s", 10),
            ("5m", 5 * 60),
            ("1h 5m", 5 * 60 + 60*60),
        ];

        for (input, num_secs) in tests {
            assert_eq!(
                Duration::parse_raw_arg(RawArgValue::String(input.to_string())).unwrap(),
                Duration::from_secs(*num_secs)
            );
        }
    }

}