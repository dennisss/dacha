/*
See https://www.debian.org/doc/debian-policy/ch-controlfields.html#version

Format: [epoch:]upstream_version[-debian_revision]
*/

use std::{cmp::Ordering, fmt::Debug};

use common::errors::*;

regexp!(VERSION_PATTERN => "^(?:([0-9]+):)?([0-9][-.+~A-Za-z0-9]*?)(?:-([.~+A-Za-z0-9]+))?$");

regexp!(DIGIT_PATTERN => "^[0-9]*");
regexp!(NONDIGIT_PATTERN => "^[^0-9]*");

#[derive(Clone)]
pub struct Version {
    pub epoch: Option<usize>,
    pub upstream_version: String,
    pub debian_revision: Option<String>,
}

impl Version {
    pub fn parse_from(s: &str) -> Result<Self> {
        let m = VERSION_PATTERN
            .exec(s)
            .ok_or_else(|| format_err!("Invalid version: {}", s))?;

        let epoch = match m.group_str(1) {
            Some(v) => Some(v?.parse()?),
            None => None,
        };

        let upstream_version = m.group_str(2).unwrap()?.to_string();

        let debian_revision = match m.group_str(3) {
            Some(v) => Some(v?.to_string()),
            None => None,
        };

        Ok(Self {
            epoch,
            upstream_version,
            debian_revision,
        })
    }
}

impl Debug for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let epoch = self
            .epoch
            .clone()
            .map(|v| v.to_string())
            .unwrap_or_default();

        let debian_revision = self
            .debian_revision
            .as_ref()
            .map(|v| format!(":{}", v))
            .unwrap_or_default();

        write!(f, "{}{}{}", epoch, self.upstream_version, debian_revision)
    }
}

impl Eq for Version {}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // TODO: More thoroughly test this and make sure it doesn't graph.

        let o = self.epoch.unwrap_or(0).cmp(&other.epoch.unwrap_or(0));
        if !o.is_eq() {
            return o;
        }

        let o = mixed_string_compare(&self.upstream_version, &other.upstream_version);
        if !o.is_eq() {
            return o;
        }

        let a = self
            .debian_revision
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("");
        let b = other
            .debian_revision
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("");
        mixed_string_compare(a, b)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other).unwrap().is_eq()
    }
}

fn take_regexp_prefix<'a>(
    r: &automata::regexp::vm::instance::StaticRegExp,
    s: &mut &'a str,
) -> &'a str {
    let m = r.exec(*s).unwrap();
    let i = m.last_index();
    let (p, rest) = s.split_at(i);
    *s = rest;
    p
}

fn mixed_string_compare(mut a: &str, mut b: &str) -> Ordering {
    // Compare sequences of non-digits, then sequences of non-digits, etc.
    while !a.is_empty() || !b.is_empty() {
        let a_nondigit = take_regexp_prefix(&NONDIGIT_PATTERN, &mut a);
        let b_nondigit = take_regexp_prefix(&NONDIGIT_PATTERN, &mut b);

        let o = lexical_compare(a_nondigit, b_nondigit);
        if !o.is_eq() {
            return o;
        }

        let mut a_digit = take_regexp_prefix(&DIGIT_PATTERN, &mut a).trim_start_matches('0');
        if a_digit.is_empty() {
            a_digit = "0";
        }

        // TODO: May overflow if too large.
        let a_num = a_digit.parse::<usize>().unwrap();

        let mut b_digit = take_regexp_prefix(&DIGIT_PATTERN, &mut b).trim_start_matches('0');
        if b_digit.is_empty() {
            b_digit = "0";
        }

        // TODO: May overflow if too large.
        let b_num = b_digit.parse::<usize>().unwrap();

        let o = a_num.cmp(&b_num);
        if !o.is_eq() {
            return o;
        }
    }

    Ordering::Equal
}

fn lexical_compare(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars();
    let mut b_chars = b.chars();

    loop {
        let ac = a_chars.next();
        let bc = b_chars.next();

        if ac.is_none() && bc.is_none() {
            return Ordering::Equal;
        }

        let o = lexical_compare_chars(ac, bc);
        if !o.is_eq() {
            return o;
        }
    }
}

fn lexical_compare_chars(a: Option<char>, b: Option<char>) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }

    // Tilda ('~') sorts before everything else (even a line ending)
    if a == Some('~') {
        return Ordering::Less;
    }
    if b == Some('~') {
        return Ordering::Greater;
    }

    // End of string is before valid characters.
    let a = match a {
        Some(v) => v,
        None => return Ordering::Less,
    };
    let b = match b {
        Some(v) => v,
        None => return Ordering::Greater,
    };

    // Letters are ordered before other characters.
    if a.is_alphabetic() && !b.is_alphabetic() {
        return Ordering::Less;
    }
    if !a.is_alphabetic() && b.is_alphabetic() {
        return Ordering::Greater;
    }

    a.cmp(&b)
}

#[cfg(test)]
mod tests {
    use crate::version;

    use super::*;

    #[test]
    fn parse_valid_versions() {
        let v = Version::parse_from("5.55-3.1+rpt2").unwrap();
        assert_eq!(v.epoch, None);
        assert_eq!(&v.upstream_version, "5.55");
        assert_eq!(
            v.debian_revision.as_ref().map(|s| s.as_str()),
            Some("3.1+rpt2")
        );

        let v = Version::parse_from("1.76.0-1676893663").unwrap();
        assert_eq!(v.epoch, None);
        assert_eq!(&v.upstream_version, "1.76.0");
        assert_eq!(
            v.debian_revision.as_ref().map(|s| s.as_str()),
            Some("1676893663")
        );

        let v = Version::parse_from("7:4.3.5-0+deb11u1+rpt3").unwrap();
        assert_eq!(v.epoch, Some(7));
        assert_eq!(&v.upstream_version, "4.3.5");
        assert_eq!(
            v.debian_revision.as_ref().map(|s| s.as_str()),
            Some("0+deb11u1+rpt3")
        );

        let v = Version::parse_from("0~git20230125+9f08463-1").unwrap();
        assert_eq!(v.epoch, None);
        assert_eq!(&v.upstream_version, "0~git20230125+9f08463");
        assert_eq!(v.debian_revision.as_ref().map(|s| s.as_str()), Some("1"));

        let v = Version::parse_from("1-2-3-4").unwrap();
        assert_eq!(v.epoch, None);
        assert_eq!(&v.upstream_version, "1-2-3");
        assert_eq!(v.debian_revision.as_ref().map(|s| s.as_str()), Some("4"));

        let v = Version::parse_from("1.0").unwrap();
        assert_eq!(v.epoch, None);
        assert_eq!(&v.upstream_version, "1.0");
        assert_eq!(v.debian_revision.as_ref().map(|s| s.as_str()), None);
    }

    #[test]
    fn lexical_string_ordering_works() {
        // Strings in the sorted order
        let order: &[&str] = &["~~", "~~a", "~", "", "a~", "a", "aa", "ab", "b"];

        // Check all pairs compare correctly.
        for i in 0..order.len() {
            assert_eq!(lexical_compare(order[i], order[i]), Ordering::Equal);

            for j in (i + 1)..order.len() {
                assert_eq!(lexical_compare(order[i], order[j]), Ordering::Less);
                assert_eq!(lexical_compare(order[j], order[i]), Ordering::Greater);
            }
        }
    }

    #[test]
    fn compare_versions_works() {
        let versions = [
            Version::parse_from("1.0~beta1~svn1245").unwrap(),
            Version::parse_from("1.0~beta1").unwrap(),
            Version::parse_from("1.0").unwrap(),
            Version::parse_from("1.1").unwrap(),
            Version::parse_from("2.0").unwrap(),
            Version::parse_from("2.0.1").unwrap(),
            Version::parse_from("1:1.0.1").unwrap(),
        ];

        for i in 0..versions.len() {
            assert_eq!(versions[i].cmp(&versions[i]), Ordering::Equal);

            for j in (i + 1)..versions.len() {
                assert_eq!(versions[i].cmp(&versions[j]), Ordering::Less,);
                assert_eq!(versions[j].cmp(&versions[i]), Ordering::Greater);
            }
        }
    }
}
