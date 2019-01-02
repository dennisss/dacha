
/// Used in file-format superblocks
pub type FormatVersion = u32;

pub const CURRENT_FORMAT_VERSION: FormatVersion = 1;

/// Uniquely identifies this complete set of machines
pub type ClusterId = u64;

/// Identifies a single store or cache machine in the cluster
/// If the top-bit is set, then this machine is a cache server
pub type MachineId = u32;

pub type VolumeId = u32;


/// All needles in the store will start at an offset aligned to this size
/// All indexed needle offsets will be defined in units of blocks from the start of the store
pub const BLOCK_SIZE: usize = 64;

pub type BlockOffset = u32;

pub type NeedleKey = u64;

pub type NeedleAltKey = u32;

pub type Cookie = [u8; 16];


/// Number of replicas of each physical volume to create for a single logical volume
pub const NUM_REPLICAS: usize = 3;

/// Maximum size of each 
pub const ALLOCATION_SIZE: usize = 10*1024; // 10Mb for testing

/// How many multiples of the allocation size less than the total store space to leave empty
/// This space will ensure that we don't risk overprovisioning space and that we have working area to perform online compactions on the same machine
pub const ALLOCATION_RESERVED: usize = 2;


/// Amount of space on the store machine's hdd to use for storing data
/// Currently fixed but eventually dynamic based on hard drive checks and configurations
pub const STORE_MACHINE_SPACE: usize = 100*1024; // 100Mb