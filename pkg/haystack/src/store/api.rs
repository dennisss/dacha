use arrayref::*;
use base64;
use std::mem::size_of;
use bytes::Bytes;
use rand::RngCore;
use std::io::{Write, Read};
use byteorder::{WriteBytesExt, ReadBytesExt, LittleEndian};
use common::errors::*;
use crate::types::*;
use crate::paths::*;


const COOKIE_SIZE: usize = size_of::<Cookie>();


#[derive(Clone)]
pub struct NeedleChunkPath {
	pub volume_id: VolumeId,
	pub key: NeedleKey,
	pub alt_key: NeedleAltKey,
	pub cookie: CookieBuf,
}

pub const NEEDLE_CHUNK_HEADER_SIZE: usize =
	size_of::<VolumeId>() +
	size_of::<NeedleKey>() +
	size_of::<NeedleAltKey>() +
	COOKIE_SIZE +
	size_of::<NeedleSize>();

/// Represents a single needle packetized for sending for upload into a machine
#[derive(Clone)]
pub struct NeedleChunk {
	pub path: NeedleChunkPath,
	pub data: Bytes
}

impl NeedleChunk {
	pub fn write_header(&self, writer: &mut Write) -> Result<()> {
		writer.write_u32::<LittleEndian>(self.path.volume_id)?;
		writer.write_u64::<LittleEndian>(self.path.key)?;
		writer.write_u32::<LittleEndian>(self.path.alt_key)?;
		writer.write_all(self.path.cookie.data())?;
		writer.write_u64::<LittleEndian>(self.data.len() as NeedleSize)?;
		Ok(())
	}

	pub fn read_header(reader: &mut Read) -> Result<(NeedleChunkPath, NeedleSize)> {
		let volume_id = reader.read_u32::<LittleEndian>()?;
		let key = reader.read_u64::<LittleEndian>()?;
		let alt_key = reader.read_u32::<LittleEndian>()?;

		let mut cookie = vec![];
		cookie.resize(COOKIE_SIZE, 0);
		reader.read_exact(&mut cookie)?;

		let size = reader.read_u64::<LittleEndian>()?;

		Ok((NeedleChunkPath {
			volume_id, key, alt_key, cookie: CookieBuf::from(Bytes::from(cookie))
		}, size))
	}
}




#[derive(Clone)]
pub struct CookieBuf {
	inner: Bytes
}

impl From<&[u8]> for CookieBuf {
	/// Creates a cookie from a byte array
	/// NOTE: No attempt is made to validate the length right here
	fn from(data: &[u8]) -> CookieBuf {
		CookieBuf {
			inner: Bytes::from(data)
		}
	}
}

impl From<Vec<u8>> for CookieBuf {
	fn from(data: Vec<u8>) -> CookieBuf { CookieBuf { inner: Bytes::from(data) } }
}

impl From<Bytes> for CookieBuf {
	fn from(data: Bytes) -> CookieBuf { CookieBuf { inner: data } }
}

impl CookieBuf {

	pub async fn random() -> Result<CookieBuf> {
		let mut arr = Vec::new(); arr.resize(COOKIE_SIZE, 0);
		crypto::random::secure_random_bytes(&mut arr).await?;
		Ok(CookieBuf::from(arr))
	}

	pub fn data(&self) -> &Cookie {
		array_ref![self.inner, 0, COOKIE_SIZE]
	}

	pub fn to_string(&self) -> String {
		serialize_urlbase64(&self.inner).trim_end_matches('=').to_string()
	}
}

impl std::str::FromStr for CookieBuf {
	type Err = base64::DecodeError;
	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		let buf = parse_urlbase64(s)?;
		if buf.len() != COOKIE_SIZE {
			return Err(base64::DecodeError::InvalidLength);
		}

		Ok(CookieBuf {
			inner: Bytes::from(buf)
		})
	}
}


pub struct ETag {
	pub store_id: MachineId,
	pub volume_id: VolumeId,
	pub block_offset: BlockOffset,
	pub checksum: Bytes
}

impl ETag {
	pub fn from(s: &str) -> Result<ETag> {
		if s.len() < 2 {
			return Err(err_msg("Too small"));
		}

		if &s[0..1] != "\"" || &s[(s.len() - 1)..] != "\"" {
			return Err(err_msg("No quotes"));
		}

		let parts = s[1..(s.len() - 1)].split(':').collect::<Vec<_>>();

		if parts.len() != 4 {
			return Err(err_msg("Not enough parts"));
		}

		let store_id = parts[0].parse::<MachineId>()
			.map_err(|_| Error::from("Invalid store id"))?;
		let volume_id = parts[1].parse::<VolumeId>()
			.map_err(|_| Error::from("Invalid volume id"))?;
		let block_offset = parts[2].parse::<BlockOffset>()
			.map_err(|_| Error::from("Invalid block offset"))?;
		let checksum = parse_urlbase64(parts[3])
			.map_err(|_| Error::from("Invalid checksum"))?;

		Ok(ETag {
			store_id,
			volume_id,
			block_offset,
			checksum: checksum.into()
		})
	}

	pub fn from_header(v: &[u8]) -> Result<ETag> {
		match std::str::from_utf8(v) {
			Ok(s) => {
				match ETag::from(s) {
					Ok(e) => Ok(e),
					Err(e) => Err(e)
				}
			},
			Err(_) => Err(err_msg("Invalid header value string"))
		}
	}

	pub fn partial_matches(&self, store_id: MachineId, volume_id: VolumeId,
						   block_offset: BlockOffset) -> bool {
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
	pub fn from(segs: &[String]) -> Result<StorePath> {
		if segs.len() == 0 {
			return Ok(StorePath::Index);
		}

		let volume_id = match segs[0].parse::<VolumeId>() {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Invalid volume id"))
		};

		if segs.len() == 1 {
			return Ok(StorePath::Volume {
				volume_id
			});
		}

		let key = match segs[1].parse::<NeedleKey>() {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Invalid needle key"))
		};
		
		if segs.len() == 2 {
			return Ok(StorePath::Photo {
				volume_id, key
			});
		}

		let alt_key = match segs[2].parse::<NeedleAltKey>() {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Invalid needle alt key"))
		};

		if segs.len() == 3 {
			return Ok(StorePath::Partial {
				volume_id, key, alt_key
			});
		}

		let cookie = match segs[3].parse::<CookieBuf>() {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Invalid cookie"))
		};

		if segs.len() == 4 {
			return Ok(StorePath::Needle {
				volume_id, key, alt_key, cookie
			});
		}

		Err(err_msg("Unknown route pattern"))
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


#[derive(Serialize, Deserialize, Debug)]
pub struct StoreError {
	pub code: u16,
	pub message: String
}

#[derive(Serialize, Deserialize)]
pub struct StoreWriteBatchResponse {
	pub num_written: usize, // Number of needle chunks of those received that were successfully 
	pub error: Option<StoreError> // If present than this error occured while writing further chunks beyond those counted in num_written
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cookie_buf() {
		let test_str = "AZCM6IJeXvYDtS715kNGEQ";
		let c = test_str.parse::<CookieBuf>();
		assert!(c.is_ok());
		assert_eq!(test_str, c.unwrap().to_string());

		let bad = "asdsd".parse::<CookieBuf>();
		assert!(bad.is_err());
	}
}

