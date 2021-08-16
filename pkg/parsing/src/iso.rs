use common::bytes::Bytes;
use common::errors::*;

/// String of Latin-1 (ISO-8859-1) encoded characters.
/// Not to be confused with 'ISO 8859-1' (without the extra hypen).
///
/// This is single-byte encoding used as the default in webpages.
/// Byte values 0-127 is identical to US-ASCII.
/// Byte values 128-255 interpreted as raw u8's as unicode code points.
pub struct Latin1String {
    // TODO: Disaallow direct assess to
    pub data: Bytes,
}

impl Latin1String {
    // TODO: Differentiate the naming of these as one converts to iso and one from
    // iso sort of?

    /// Convert an str of unicode characters to an ISO string.
    /// This will fail if the codepoints don't fit in a single byte.
    pub fn from(s: &str) -> Result<Latin1String> {
        let mut data = vec![];
        for c in s.chars() {
            let v = c as usize;
            if v > 0xff {
                return Err(err_msg("Char outside of single byte range"));
            }

            data.push(v as u8);
        }

        Latin1String::from_bytes(Bytes::from(data))
    }

    /// Create an object wrapping bytes encoded in ISO format.
    pub fn from_bytes(data: Bytes) -> Result<Latin1String> {
        // for i in &data {
        // 	let valid = (/* *i >= 0x20 && */ *i <= 0x7e) ||
        // 				(*i >= 0xa0);
        // 	if !valid {
        // 		return Err(
        // 			format!("Undefined ISO-8859-1 code point: {:x}", i).into());
        // 	}
        // }

        Ok(Latin1String { data })
    }

    /// Converts to a standard utf-8 string.
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        for i in &self.data {
            let c = std::char::from_u32(*i as u32).expect("Invalid character");
            s.push(c);
        }

        s
    }

    pub fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }

    // TODO: Add an as_ascii_str() for the case of the string using the ASCII string
    // subset.
}

impl std::fmt::Debug for Latin1String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_string().fmt(f)
    }
}
