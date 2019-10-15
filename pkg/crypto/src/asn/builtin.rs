// Implementations of builtin types for use in compiled code.

use common::errors::*;
use common::bits::BitVector;
use parsing::ascii::AsciiString;
use bytes::Bytes;


pub struct Any {
	pub data: Bytes
}

pub struct SequenceOf<T> {
	items: Vec<T>
}

pub struct SetOf<T> {
	items: Vec<T>
}


pub struct UTF8String {
	pub data: Bytes
}

pub struct PrintableString(AsciiString);

pub struct ObjectIdentifier {
	components: Vec<usize>
}

impl ObjectIdentifier {
	pub fn new() -> Self {
		Self { components: vec![] }
	}

	pub fn extend<T: AsRef<[usize]>>(mut self, vals: T) -> Self {
		self.components.extend_from_slice(vals.as_ref());
		self
	}

	// pub fn from_str(s: &str) -> Self {
	// 	// Parse using the 
	// }

}

// TODO: Generally we always need to check values to ensure that the value is
// at least the right type.
// pub trait ConstrainedType {
// 	fn check_vale(&self) -> 
// }


pub struct BitString {
	/// TODO: Need a BitVector that can implement zero copy via a Bytes object.
	pub data: BitVector
}

pub struct OctetString {
	pub data: Bytes
}


// TeletexString

// PrintableString

// UniversalString

// UTF8String

// BMPString

pub struct IA5String {
	pub data: AsciiString
}

// Any

// GeneralizedTime
pub struct GeneralizedTime {
	
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

// Time


// YYMMDDhhmmZ
// YYMMDDhhmm+hh'mm'
// YYMMDDhhmm-hh'mm'
// YYMMDDhhmmssZ
// YYMMDDhhmmss+hh'mm'
// YYMMDDhhmmss-hh'mm'
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
	pub timezone: Option<isize>
}

impl UTCTime {

	pub fn to_string(&self) -> String {
		// TODO: Validate that the stored values are in range

		let secs =
			if let Some(v) = self.seconds {
				format!("{:02}", v)
			} else { String::new() };
		let timezone =
			if let Some(v) = self.timezone {
				let m = v.abs();
				let s = if v >= 0 { '+' } else { '-' };
				let mm = m % 60;
				let hh = m / 60;
				format!("{}{:02}'{:02}'", s, hh, mm)
			} else { "Z".into() };

		format!("{:02}{:02}{:02}{:02}{:02}{}{}", self.year_short, self.month,
				self.day, self.hour, self.minute, secs, timezone)
	}

	pub fn from_str(s: &str) -> Result<UTCTime> {
		// TODO: Convert to a regex based parser.
		if s.len() < 11 {
			return Err("UTCTime string too short".into());
		}

		let year_short = u8::from_str_radix(&s[0..2], 10)?;
		let month = u8::from_str_radix(&s[2..4], 10)?;
		let day = u8::from_str_radix(&s[4..6], 10)?;
		let hour = u8::from_str_radix(&s[6..8], 10)?;
		let minute = u8::from_str_radix(&s[8..10], 10)?;

		if month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 ||
			minute > 59 {
			return Err("Time component out of range".into());
		}

		let mut next_idx = 11;
		let seconds =
			if s.chars().nth(next_idx).unwrap().is_digit(10) {
				if s.len() < next_idx + 2 {
					return Err("Too short".into());
				}

				let v = u8::from_str_radix(&s[next_idx..(next_idx + 2)], 10)?;
				next_idx += 2;

				if v > 59 {
					return Err("Seconds out of range.".into());
				}

				Some(v)
			} else {
				None
			};

		if s.len() < next_idx + 1 {
			return Err("Missing timezone".into());
		}

		let timezone_char = s.chars().nth(next_idx).unwrap();
		let timezone =
			if timezone_char == 'Z' {
				next_idx += 1;
				None
			} else {
				// +hh'mm'
				if s.len() < next_idx + 7 {
					return Err("Invalid timezone".into());
				}

				let sign = match timezone_char {
					'+' => 1,
					'-' => -1,
					_ => { return Err("Invalid timezone sign".into()); }
				};

				let hh = u8::from_str_radix(
					&s[(next_idx + 1)..(next_idx + 3)], 10)?;
				let mm = u8::from_str_radix(
					&s[(next_idx + 4)..(next_idx + 6)], 10)?;


				if hh > 23 || mm > 59 ||
					s.chars().nth(next_idx + 3).unwrap() != '\'' ||
					s.chars().nth(next_idx + 6).unwrap() != '\'' {
					return Err("Out of range timezone".into())
				}

				Some(sign*((hh as isize)*60 + (mm as isize)))
			};

		if next_idx != s.len() {
			return Err("Timestamp too long".into());
		}

		Ok(Self { year_short, month, day, hour, minute, seconds, timezone })
	}
}
