
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
