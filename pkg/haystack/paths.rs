use super::common::*;
use arrayref::*;
use base64;
use std::mem::size_of;
use bytes::Bytes;

const COOKIE_SIZE: usize = size_of::<Cookie>();


pub struct CookieBuf {
	inner: Bytes
}

impl CookieBuf {

	/// Creates a cookie from a byte array
	/// NOTE: No attempt is made to validate the length right here
	pub fn from(data: &[u8]) -> CookieBuf {
		CookieBuf {
			inner: Bytes::from(data)
		}
	}

	pub fn data(&self) -> &Cookie {
		array_ref!(self.inner, 0, COOKIE_SIZE)
	}

	pub fn to_string(&self) -> String {
		serialize_urlbase64(&self.inner).trim_end_matches('=').to_string()
	}
}

impl std::str::FromStr for CookieBuf {
	type Err = base64::DecodeError;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let buf = parse_urlbase64(s)?;
		if buf.len() != COOKIE_SIZE {
			return Err(base64::DecodeError::InvalidLength);
		}

		Ok(CookieBuf {
			inner: Bytes::from(buf)
		})
	}
}


pub enum MachineIds {
	Data(Vec<MachineId>),

	/// The meaning of this will depend on the method used on the cache but will generally mean that the cache is free to choose which machines the request should be forwarded to
	Unspecified
}

impl std::str::FromStr for MachineIds {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut list = vec![];

		for part in s.split('-').into_iter() {
			match part.parse::<MachineId>() {
				Ok(v) => list.push(v),
				Err(_) => return Err("Contains invalid ids")
			};
		}

		Ok(MachineIds::Data(list))

	}
}

pub fn parse_urlbase64(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
	base64::decode_config(s, base64::URL_SAFE)
}

pub fn serialize_urlbase64(s: &[u8]) -> String {
	base64::encode_config(s, base64::URL_SAFE)
}


pub fn split_path_segments(path: &str) -> Option<Vec<String>> {
	let mut segs: Vec<String> = path.split('/').into_iter().map(|s| { String::from(s) }).collect();

	if segs.len() < 2 || &segs[0] != "" {
		return None;
	}

	// Special case of the index route that takes up no segments
	if &segs[1] == "" {
		return Some(vec![]);
	}

	// Remove the trivial '' before the first slash and return it
	segs.remove(0);

	Some(segs)
}

pub enum StorePath {
	/// '/' 
	Index,

	/// '/<volume_id>'
	Volume {
		volume_id: VolumeId
	},

	/// '/<volume_id>/<key>'
	Photo {
		volume_id: VolumeId,
		key: NeedleKey
	},

	/// '/<volume_id>/<key>/<alt_key>'
	Partial {
		volume_id: VolumeId,
		key: NeedleKey,
		alt_key: NeedleAltKey
	},

	/// '/<volume_id>/<key>/<alt_key>/<cookie>'
	Needle {
		volume_id: VolumeId,
		key: NeedleKey,
		alt_key: NeedleAltKey,
		cookie: CookieBuf
	},

	Invalid
}

impl StorePath {
	pub fn from(segs: &[String]) -> StorePath {
		if segs.len() == 0 {
			return StorePath::Index;
		}

		let volume_id = match segs[0].parse::<VolumeId>() {
			Ok(v) => v,
			Err(_) => return StorePath::Invalid
		};

		if segs.len() == 1 {
			return StorePath::Volume {
				volume_id
			};
		}

		let key = match segs[1].parse::<NeedleKey>() {
			Ok(v) => v,
			Err(_) => return StorePath::Invalid
		};
		
		if segs.len() == 2 {
			return StorePath::Photo {
				volume_id, key
			};
		}

		let alt_key = match segs[2].parse::<NeedleAltKey>() {
			Ok(v) => v,
			Err(_) => return StorePath::Invalid
		};

		if segs.len() == 3 {
			return StorePath::Partial {
				volume_id, key, alt_key
			};
		}

		let cookie = match segs[3].parse::<CookieBuf>() {
			Ok(v) => v,
			Err(_) => return StorePath::Invalid
		};

		if segs.len() == 4 {
			return StorePath::Needle {
				volume_id, key, alt_key, cookie
			};
		}

		StorePath::Invalid
	}
}
