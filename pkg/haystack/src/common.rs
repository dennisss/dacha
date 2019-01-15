
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

#[derive(Deserialize)]
#[serde(default)]
pub struct StoreConfig {
	/// Number of replicas of each physical volume to create for a single logical volume
	pub num_replicas: usize,

	/// Amount of space on the store machine's hdd to use for storing data
	/// Currently fixed but eventually dynamic based on hard drive checks and configurations
	pub space: u64,

	/// All needles in the store will start at an offset aligned to this size
	/// All indexed needle offsets will be defined in units of blocks from the start of the store
	/// NOTE: Once a physical volume is created with this size, it will stay that way until compacted
	pub block_size: u64,

	/// Maximum size of each volume on a store machine
	pub allocation_size: u64,

	/// How many multiples of the allocation size less than the total store space to leave empty
	/// This space will ensure that we don't risk overprovisioning space and that we have working area to perform online compactions on the same machine
	pub allocation_reserved: usize,

	/// Block size for allocations done with the filesystem. We will ensure that the amount of blocks in the filesystem allocated towards one store file is aligned aligned up to multiples of this byte size. This is used to ensure that store files use mostly contiguous blocks on the disk when possible (Facebook's paper uses 1GB)
	/// TODO: Probably good to validate this against the fs2::allocation_granularity for a volume
	pub preallocate_size: u64,

	pub heartbeat_interval: u64,

	/// Must get a heartbeat with-in this amount of time to be considering alive and well
	pub heartbeat_timeout: u64
}

impl Default for StoreConfig {
	fn default() -> Self {
		StoreConfig {
			num_replicas: 3,
			block_size: 64,
			allocation_size: 100*1024*1024, // 100MB for testing
			allocation_reserved: 2,
			preallocate_size: 1*1024*1024, // 1MB for testing
			space: 1024*1024*1024, // 1GB
			heartbeat_interval: 10000, // Heartbeat send every 10 seconds
			heartbeat_timeout: 30000
		}
	}
}

#[derive(Deserialize)]
#[serde(default)]
pub struct CacheConfig {
	pub memory_size: usize,
	pub max_age: u64,
	pub max_entry_size: usize
}

impl Default for CacheConfig {
	fn default() -> Self {
		CacheConfig {
			memory_size: 100*1024, // 100Mb of in-memory caching
			max_age: 60*60*1000, // 1 hour before the cache must be invalidated
			max_entry_size: 10*1024
		}
	}
}


#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {

	pub store: StoreConfig,

	pub cache: CacheConfig

	// TODO: Probably also move the directory config into here as well

}

pub type ConfigRef = std::sync::Arc<Config>;


