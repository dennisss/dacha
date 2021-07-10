use crate::proto::config::Config;

/// Used in file-format superblocks
pub type FormatVersion = u32;

pub const CURRENT_FORMAT_VERSION: FormatVersion = 1;

/// Uniquely identifies this complete set of machines
pub type ClusterId = u64;

/// Identifies a single store or cache machine in the cluster
/// If the top-bit is set, then this machine is a cache server
pub type MachineId = u32;

pub type VolumeId = u32;

/// NOTE: This is mainly the size as stored on disk (in memory we will hold this as a u64 for more convenience)
pub type BlockSize = u32;

pub type BlockOffset = u32;

pub type NeedleKey = u64;

pub type NeedleAltKey = u32;

pub type NeedleSize = u64;

pub type Cookie = [u8; 16];


#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NeedleKeys {
	pub key: NeedleKey,
	pub alt_key: NeedleAltKey
}

pub type ConfigRef = std::sync::Arc<Config>;


