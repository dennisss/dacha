use super::common::*;
use arrayref::*;
use base64;
use std::mem::size_of;
use bytes::Bytes;

const COOKIE_SIZE: usize = size_of::<Cookie>();

#[derive(Clone)]
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

impl MachineIds {
	pub fn to_string(&self) -> String {
		match self {
			// TODO: Must have at least one element for this to be valid
			MachineIds::Data(arr) =>
				arr.iter().map(|id| id.to_string()).collect::<Vec<String>>().join("-"),
			MachineIds::Unspecified =>
				"-".into()
		}
	}
}

impl std::str::FromStr for MachineIds {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s == "-" {
			return Ok(MachineIds::Unspecified);
		}

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

/// NOTE: For all paths, aside from the index route of the base path, trailing slashes we consider to be invalid
#[derive(Clone)]
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
	}
}

impl StorePath {
	pub fn from(segs: &[String]) -> Result<StorePath, &'static str> {
		if segs.len() == 0 {
			return Ok(StorePath::Index);
		}

		let volume_id = match segs[0].parse::<VolumeId>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid volume id")
		};

		if segs.len() == 1 {
			return Ok(StorePath::Volume {
				volume_id
			});
		}

		let key = match segs[1].parse::<NeedleKey>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid needle key")
		};
		
		if segs.len() == 2 {
			return Ok(StorePath::Photo {
				volume_id, key
			});
		}

		let alt_key = match segs[2].parse::<NeedleAltKey>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid needle alt key")
		};

		if segs.len() == 3 {
			return Ok(StorePath::Partial {
				volume_id, key, alt_key
			});
		}

		let cookie = match segs[3].parse::<CookieBuf>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid cookie")
		};

		if segs.len() == 4 {
			return Ok(StorePath::Needle {
				volume_id, key, alt_key, cookie
			});
		}

		Err("Unknown route pattern")
	}

	pub fn to_string(&self) -> String {
		match self {
			StorePath::Index => "/".into(),
			StorePath::Volume { volume_id } =>
				format!("/{}", volume_id),
			StorePath::Photo { volume_id, key } =>
				format!("/{}/{}", volume_id, key),
			StorePath::Partial { volume_id, key, alt_key } => 
				format!("/{}/{}/{}", volume_id, key, alt_key),
			StorePath::Needle { volume_id, key, alt_key, cookie } => 
				format!("/{}/{}/{}/{}", volume_id, key, alt_key, cookie.to_string()) 
		}
	}
}


pub enum CachePath {
	// '/'
	Index,

	// '/<machine_ids>/<some_valid_store_path>'
	Proxy {
		machine_ids: MachineIds,
		store: StorePath
	}
}

impl CachePath {
	pub fn from(segs: &[String]) -> std::result::Result<CachePath, &'static str> {
		if segs.len() == 0 {
			return Ok(CachePath::Index);
		}

		let machine_ids = match segs[0].parse::<MachineIds>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid machine ids")
		};

		let store = StorePath::from(&segs[1..])?;

		Ok(CachePath::Proxy {
			machine_ids,
			store
		})
	}

	pub fn to_string(&self) -> String {
		match self {
			CachePath::Index => "/".into(),
			CachePath::Proxy { machine_ids, store } => 
				format!("/{}/{}", machine_ids.to_string(), store.to_string())
		}
	}
}


pub struct ETag {
	pub store_id: MachineId,
	pub volume_id: VolumeId,
	pub block_offset: BlockOffset,
	pub checksum: Bytes
}

impl ETag {

	pub fn from(s: &str) -> std::result::Result<ETag, &'static str> {
		if s.len() < 2 {
			return Err("Too small");
		}

		if &s[0..1] != "\"" || &s[(s.len() - 1)..] != "\"" {
			return Err("No quotes");
		}

		let parts = s[1..(s.len() - 1)].split(':').collect::<Vec<_>>();

		if parts.len() != 4 {
			return Err("Not enough parts");
		}

		let store_id = parts[0].parse::<MachineId>().map_err(|_| "Invalid store id")?;
		let volume_id = parts[1].parse::<VolumeId>().map_err(|_| "Invalid volume id")?;
		let block_offset = parts[2].parse::<BlockOffset>().map_err(|_| "Invalid block offset")?;
		let checksum = parse_urlbase64(parts[3]).map_err(|_| "Invalid checksum")?;

		Ok(ETag {
			store_id,
			volume_id,
			block_offset,
			checksum: checksum.into()
		})
	}

	pub fn from_header(v: &hyper::header::HeaderValue) -> std::result::Result<ETag, &'static str> {
		match v.to_str() {
			Ok(s) => {
				match ETag::from(s) {
					Ok(e) => Ok(e),
					Err(e) => Err(e)
				}
			},
			Err(_) => Err("Invalid header value string")
		}
	}

	pub fn partial_matches(&self, store_id: MachineId, volume_id: VolumeId, block_offset: BlockOffset) -> bool {
		self.store_id == store_id && self.volume_id == volume_id && self.block_offset == block_offset
	}

	pub fn matches(&self, other: &ETag) -> bool {
		// Small safety check against potential CRC32 collisions
		// If the other etag came from this machine, then we know for sure if it was modified based on the monotonic offsets
		// (this case should be dominant the majority of the time between the cache and the backend store)		
		if self.store_id == other.store_id && self.volume_id == other.volume_id {
			return self.block_offset == other.block_offset;
		}

		&self.checksum == &other.checksum
	}

	pub fn to_string(&self) -> String {
		format!(
			"\"{}:{}:{}:{}\"",
			self.store_id,
			self.volume_id,
			self.block_offset,
			serialize_urlbase64(&self.checksum).trim_end_matches('=')
		)
	}
}

