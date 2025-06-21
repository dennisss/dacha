use std::time::Duration;
use alloc::string::String;

struct Unit(&'static str, u64, u64);

const UNITS: &'static [Unit] = &[
    Unit("s", 1, 1),
    Unit("m", 1, 60),
    Unit("h", 1, 60*60),
    Unit("d", 1, 60*60*24),
    Unit("y", 1, 60*60*24*365),
];

/// Formats a duration with up to single second accuracy.
pub fn format_duration_secs(v: Duration) -> String {
    let mut v = v.as_secs();

    let mut out = String::new();

    for (i, unit) in UNITS.iter().rev().enumerate() {

        let n = v / unit.2;
        v %= unit.2;

        if n > 0 || (out.is_empty() && i == UNITS.len() - 1) {
            if !out.is_empty() {
                out.push(' ');
            }

            out.push_str(&format!("{}{}", n, unit.0));
        }
    }

    out
}
