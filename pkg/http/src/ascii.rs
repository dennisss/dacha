use common::errors::*;
use bytes::Bytes;

/// An str backed by bytes that we know only contain bytes 0-127.
pub struct AsciiString {
	pub data: Bytes
}

impl AsciiString {
	pub fn from<T: AsRef<[u8]>>(data: T) -> Result<AsciiString> {
		let d = data.as_ref();
		let mut out = vec![];
		for v in data.as_ref().iter().cloned() {
			if v > 127 {
				return Err("Byte outside of ASCII range".into());
			}

			out.push(v);
		}

		Ok(AsciiString { data: Bytes::from(out) })
	}

	pub unsafe fn from_ascii_unchecked(data: Bytes) -> AsciiString {
		AsciiString { data }
	}
	pub fn eq_ignore_case(&self, other: &[u8]) -> bool {
		self.data.eq_ignore_ascii_case(other)
	}
	pub fn to_string(&self) -> String {
		self.as_ref().to_owned()
	}
}

impl std::convert::AsRef<str> for AsciiString {
	fn as_ref(&self) -> &str {
		unsafe { std::str::from_utf8_unchecked(&self.data) }
	}
}

impl std::fmt::Debug for AsciiString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", std::convert::AsRef::<str>::as_ref(self))
    }
}