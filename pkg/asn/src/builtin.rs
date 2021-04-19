// Implementations of builtin types for use in compiled code.

use std::clone::Clone;
use std::convert::AsRef;
use std::fmt::Debug;
use std::string::ToString;

use common::chrono::{Date, DateTime, FixedOffset, TimeZone, Utc};
use common::bits::BitVector;
use common::bytes::Bytes;
use common::errors::*;
use common::vec::VecPtr;
use parsing::ascii::AsciiString;

// use super::encoding::Element;

/// A wrapper around Bytes which can be statically initialized and this can't
/// be mutated without cloning.
#[derive(Clone)]
pub enum BytesRef {
    Static(&'static [u8]),
    Dynamic(Bytes),
}

impl BytesRef {
    pub const fn from_static(data: &'static [u8]) -> Self {
        Self::Static(data)
    }

    pub fn clone_inner(&self) -> Bytes {
        self.clone().into()
    }
}

impl std::fmt::Debug for BytesRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}",
            match self {
                Self::Static(v) => Bytes::from_static(v),
                // TODO: Optimize this (we mainly use bytes to use the smarter debug
                // formatter)
                Self::Dynamic(v) => v.clone(),
            }
        )
    }
}

impl std::convert::AsRef<[u8]> for BytesRef {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Static(v) => v,
            Self::Dynamic(v) => &v,
        }
    }
}

impl std::convert::Into<Bytes> for BytesRef {
    fn into(self) -> Bytes {
        match self {
            Self::Static(v) => Bytes::from_static(v),
            Self::Dynamic(v) => v,
        }
    }
}

impl std::convert::From<Bytes> for BytesRef {
    fn from(data: Bytes) -> Self {
        Self::Dynamic(data)
    }
}

impl std::cmp::PartialEq for BytesRef {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Null {}

impl Null {
    pub const fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SequenceOf<T> {
    pub items: Vec<T>,
}

impl<T: Debug + Clone> AsRef<[T]> for SequenceOf<T> {
    fn as_ref(&self) -> &[T] {
        &self.items
    }
}

// TODO: Comparison of these must be pairwise (unless we gurantee that they are
// sorted.
#[derive(Debug, Clone)]
pub struct SetOf<T> {
    pub items: Vec<T>,
}

impl<T: Debug + Clone> AsRef<[T]> for SetOf<T> {
    fn as_ref(&self) -> &[T] {
        &self.items
    }
}

#[derive(Debug, Clone)]
pub struct PrintableString(pub AsciiString);

impl ToString for PrintableString {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(PartialEq, Eq, Clone)]
pub struct ObjectIdentifier {
    components: VecPtr<usize>,
}

impl ObjectIdentifier {
    // TODO: Generate these using Into<VecPtr<usize>>

    /// Creates an ObjectIdentifier backed by a static array. Meant for top
    /// level declarations of compiled values.
    /// TODO: Rename from_static
    pub const fn from(components: &'static [usize]) -> Self {
        Self {
            components: VecPtr::from_static(components),
        }
    }

    pub fn from_vec(components: Vec<usize>) -> Self {
        Self {
            components: VecPtr::from_vec(components),
        }
    }

    pub fn new() -> Self {
        Self {
            components: VecPtr::new(),
        }
    }

    // pub fn extend<T: AsRef<[usize]>>(mut self, vals: T) -> Self {
    // 	self.components.extend_from_slice(vals.as_ref());
    // 	self
    // }

    // pub fn from_str(s: &str) -> Self {
    // 	// Parse using the
    // }
}

// TODO: Move to VecPtr.
impl std::hash::Hash for ObjectIdentifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.components.as_ref().hash(state);
    }
}

impl AsRef<[usize]> for ObjectIdentifier {
    fn as_ref(&self) -> &[usize] {
        self.components.as_ref()
    }
}

impl std::fmt::Debug for ObjectIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let items = self.components.as_ref();
        write!(
            f,
            "[{}]",
            items
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(".")
        )
    }
}

#[macro_export]
macro_rules! oid {
	($( $e:expr ),*) => {
		ObjectIdentifier::from(&[$($e),*])
	};
}

// TODO: Generally we always need to check values to ensure that the value is
// at least the right type.
// pub trait ConstrainedType {
// 	fn check_vale(&self) ->
// }

#[derive(Debug, Clone)]
pub struct BitString {
    /// TODO: Need a BitVector that can implement zero copy via a Bytes object.
    pub data: BitVector,
}

impl std::ops::Deref for BitString {
    type Target = BitVector;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl std::convert::Into<BitVector> for BitString {
    fn into(self) -> BitVector {
        self.data
    }
}

// TODO:
#[derive(Debug, Clone, PartialEq)]
pub struct OctetString(pub BytesRef);

impl OctetString {
    pub const fn from_static(data: &'static [u8]) -> Self {
        Self(BytesRef::from_static(data))
    }

    pub fn to_bytes(&self) -> Bytes {
        self.0.clone_inner()
    }

    pub fn into_bytes(self) -> Bytes {
        self.0.into()
    }
}

impl<T: std::convert::Into<Bytes>> std::convert::From<T> for OctetString {
    fn from(value: T) -> Self {
        Self(value.into().into())
    }
}

impl std::ops::Deref for OctetString {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl std::convert::AsRef<[u8]> for OctetString {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct NumericString {}

impl ToString for NumericString {
    // TODO
    fn to_string(&self) -> String {
        String::new()
    }
}

#[derive(Debug, Clone)]
pub struct VisibleString {}

impl ToString for VisibleString {
    // TODO
    fn to_string(&self) -> String {
        String::new()
    }
}

// TeletexString
#[derive(Debug, Clone)]
pub struct TeletexString {
    pub data: String,
}

impl ToString for TeletexString {
    fn to_string(&self) -> String {
        self.data.clone()
    }
}

pub type T61String = TeletexString;

// UniversalString
#[derive(Debug, Clone)]
pub struct UniversalString {
    pub data: String,
}

impl ToString for UniversalString {
    fn to_string(&self) -> String {
        self.data.clone()
    }
}

// UTF8String
#[derive(Clone)]
pub struct UTF8String {
    data: Bytes,
}

impl UTF8String {
    pub fn from(data: Bytes) -> Result<Self> {
        std::str::from_utf8(&data)?;
        Ok(Self { data })
    }
}

impl std::convert::AsRef<str> for UTF8String {
    fn as_ref(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.data) }
    }
}

impl ToString for UTF8String {
    fn to_string(&self) -> String {
        AsRef::<str>::as_ref(self).to_string()
    }
}

impl std::convert::AsRef<[u8]> for UTF8String {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl std::fmt::Debug for UTF8String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", AsRef::<str>::as_ref(self))
    }
}

// BMPString
#[derive(Debug, Clone)]
pub struct BMPString {
    pub data: String,
}

impl ToString for BMPString {
    fn to_string(&self) -> String {
        self.data.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct IA5String {
    pub data: AsciiString,
}

impl ToString for IA5String {
    fn to_string(&self) -> String {
        self.data.to_string()
    }
}

// Any

// Time

// NOTE: A year of 50 is 1950 and 49 is 2049 (at least for X509)
//
// YYMMDDhhmmZ
// YYMMDDhhmm+hh'mm'
// YYMMDDhhmm-hh'mm'
// YYMMDDhhmmssZ
// YYMMDDhhmmss+hh'mm'
// YYMMDDhhmmss-hh'mm'
#[derive(Debug, Clone)]
pub struct UTCTime {
    /// Least significant two digits of the year.
    pub year_short: u8,

    /// Month from 1 to 12.
    pub month: u8,

    /// From 1 to 31
    pub day: u8,

    /// From 0 to 23
    pub hour: u8,

    // From 0 to 59
    pub minute: u8,

    // From 0 to 59
    pub seconds: Option<u8>,

    /// Timezone in number of minutes relative to GMT. If not given, then it is
    /// using GMT time.
    pub timezone: Option<isize>,
}

impl UTCTime {
    pub fn to_datetime(&self) -> DateTime<FixedOffset> {
        let year = if self.year_short < 50 {
            2000 + (self.year_short as i32)
        } else {
            1900 + (self.year_short as i32)
        };

        FixedOffset::east((60 * self.timezone.unwrap_or(0)) as i32)
            .ymd(year, self.month.into(), self.day.into())
            .and_hms(
                self.hour.into(),
                self.minute.into(),
                self.seconds.unwrap_or(0).into(),
            )
    }

    pub fn to_string(&self) -> String {
        // TODO: Validate that the stored values are in range

        let secs = if let Some(v) = self.seconds {
            format!("{:02}", v)
        } else {
            String::new()
        };
        let timezone = if let Some(v) = self.timezone {
            let m = v.abs();
            let s = if v >= 0 { '+' } else { '-' };
            let mm = m % 60;
            let hh = m / 60;
            format!("{}{:02}'{:02}'", s, hh, mm)
        } else {
            "Z".into()
        };

        format!(
            "{:02}{:02}{:02}{:02}{:02}{}{}",
            self.year_short, self.month, self.day, self.hour, self.minute, secs, timezone
        )
    }

    pub fn from_str(s: &str) -> Result<UTCTime> {
        // println!("UTCTime: {}", s);

        // TODO: Convert to a regex based parser.
        if s.len() < 11 {
            return Err(err_msg("UTCTime string too short"));
        }

        let year_short = u8::from_str_radix(&s[0..2], 10)?;
        let month = u8::from_str_radix(&s[2..4], 10)?;
        let day = u8::from_str_radix(&s[4..6], 10)?;
        let hour = u8::from_str_radix(&s[6..8], 10)?;
        let minute = u8::from_str_radix(&s[8..10], 10)?;

        // YYMMDD000000Z
        // 190905202147Z
        // YYMMDDhhmmZ

        if month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 || minute > 59 {
            return Err(err_msg("Time component out of range"));
        }

        let mut next_idx = 10;
        let seconds = if s.chars().nth(next_idx).unwrap().is_digit(10) {
            if s.len() < next_idx + 2 {
                return Err(err_msg("Too short"));
            }

            let v = u8::from_str_radix(&s[next_idx..(next_idx + 2)], 10)?;
            next_idx += 2;

            if v > 59 {
                return Err(err_msg("Seconds out of range."));
            }

            Some(v)
        } else {
            None
        };

        if s.len() < next_idx + 1 {
            return Err(err_msg("Missing timezone"));
        }

        let timezone_char = s.chars().nth(next_idx).unwrap();
        let timezone = if timezone_char == 'Z' {
            next_idx += 1;
            None
        } else {
            // +hh'mm'
            if s.len() < next_idx + 7 {
                return Err(err_msg("Invalid timezone"));
            }

            let sign = match timezone_char {
                '+' => 1,
                '-' => -1,
                _ => {
                    return Err(err_msg("Invalid timezone sign"));
                }
            };

            let hh = u8::from_str_radix(&s[(next_idx + 1)..(next_idx + 3)], 10)?;
            let mm = u8::from_str_radix(&s[(next_idx + 4)..(next_idx + 6)], 10)?;

            if hh > 23
                || mm > 59
                || s.chars().nth(next_idx + 3).unwrap() != '\''
                || s.chars().nth(next_idx + 6).unwrap() != '\''
            {
                return Err(err_msg("Out of range timezone"));
            }

            Some(sign * ((hh as isize) * 60 + (mm as isize)))
        };

        if timezone.is_some() || seconds.is_none() {
            return Err(err_msg("UTCTime Not valid DER"));
        }

        if next_idx != s.len() {
            return Err(err_msg("Timestamp too long"));
        }

        Ok(Self {
            year_short,
            month,
            day,
            hour,
            minute,
            seconds,
            timezone,
        })
    }
}

// GeneralizedTime
// YYYYMMDDhhmmss(.[0-9]*[1-9])?Z
#[derive(Debug, Clone)]
pub struct GeneralizedTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub seconds: u8,
    pub nanos: u32,
}

use std::str::FromStr;

impl GeneralizedTime {
    pub fn to_datetime(&self) -> DateTime<Utc> {
        Utc.ymd(self.year as i32, self.month as u32, self.day as u32)
            .and_hms_nano(
                self.hour as u32,
                self.minute as u32,
                self.seconds as u32,
                self.nanos as u32,
            )
    }

    pub fn to_string(&self) -> String {
        // TODO: Perform this entire function using a single string buffer of
        // with the maximum allowable length reserved.

        let nanos = format!("{:09}", self.nanos)
            .trim_end_matches('0')
            .to_string();
        let decimal = if nanos.len() == 0 {
            "".into()
        } else {
            format!(".{}", nanos)
        };
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}{}",
            self.year, self.month, self.day, self.hour, self.minute, self.seconds, decimal
        )
    }

    pub fn from_str(s: &str) -> Result<GeneralizedTime> {
        println!("GeneralizedTime: {}", s);

        if s.len() < 15 {
            return Err(err_msg("Too short"));
        }

        if s.chars().last().unwrap() != 'Z' {
            return Err(err_msg("Must end in 'Z'"));
        }

        let year = u16::from_str_radix(&s[0..4], 10)?;
        let month = u8::from_str_radix(&s[4..6], 10)?;
        let day = u8::from_str_radix(&s[6..8], 10)?;
        let hour = u8::from_str_radix(&s[8..10], 10)?;
        let minute = u8::from_str_radix(&s[10..12], 10)?;
        let seconds = u8::from_str_radix(&s[12..14], 10)?;

        if month < 1
            || month > 12
            || day < 1
            || day > 31
            || hour > 23
            || minute > 59
            || seconds > 59
        {
            return Err(err_msg("Time component out of range"));
        }

        let mut nanos = 0;
        if s.chars().nth(14).unwrap() == '.' {
            let n = s.len() - 16;
            if n == 0 {
                return Err(err_msg(". without anything following"));
            }
            if n > 9 {
                return Err(err_msg("Resolution beyond nanos not supported"));
            }
            if s.chars().nth(14 + n).unwrap() == '0' {
                return Err(err_msg("Too many right padded zeros"));
            }

            let mut num = String::from_str(&s[15..(15 + n)]).unwrap();
            while num.len() < 9 {
                num.push('0');
            }

            nanos = u32::from_str_radix(&num, 10)?;
        } else if s.len() != 15 {
            return Err(err_msg("Unknown info after seconds"));
        }

        // TODO: Validate the range of each of the numbers.

        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            seconds,
            nanos,
        })
    }
}

/*

11.7.1
The encoding shall terminate with a "Z", as described in the Rec. ITU-T X.680 | ISO/IEC 8824-1 clause
on GeneralizedTime .
11.7.2
The seconds element shall always be present.
11.7.3
The fractional-seconds elements, if present, shall omit all trailing zeros; if the elements correspond to 0,
they shall be wholly omitted, and the decimal point element also shall be omitted.
EXAMPLE
A seconds element of "26.000" shall be represented as "26"; a seconds element of "26.5200" shall be represented
as "26.52".
11.7.4 The decimal point element, if present, shall be the point option " . ".
11.7.5 Midnight (GMT) shall be represented in the form:
"YYYYMMDD000000Z"
where "YYYYMMDD" represents the day following the midnight in question.
EXAMPLE
Examples of valid representations:
"19920521000000Z"
"19920622123421Z"
"19920722132100.3Z"
Examples of invalid representations:
11.8
"19920520240000Z" (midnight represented incorrectly)
"19920622123421.0Z" (spurious trailing zeros)
"19920722132100.30Z" (spurious trailing zeros)

*/
