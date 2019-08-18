use bytes::Bytes;

/// An str backed by bytes that we know only contain bytes 0-127.
pub struct AsciiString {
	pub data: Bytes
}

impl AsciiString {
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