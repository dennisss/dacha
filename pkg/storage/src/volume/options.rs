use alloc::vec::Vec;

/// Options to use when opening a new/existing volume file system.
pub struct VolumeFSOpenOptions {
    /// Start offset in bytes of this filesystem relative to the beginning of
    /// the file descriptor provided to the VolumeFS instance.
    pub relative_start: u64,

    /// Start offset in bytes of this filesystem relative to the beginning of
    /// the disk.
    pub absolute_start: u64,

    /// Total size in bytes available in the partition storing the volume
    /// filesystem.
    pub size: u64,

    /// Key used to encrypt all unencrypted data on this disk. User encrypted
    /// data is not double encrypted.
    ///
    /// If present, then existing data on the disk MUST be encrypted (attempting
    /// to read plaintext data will return an error).
    pub encryption_key: Option<Vec<u8>>,

    /// Whether or not we should send discard (TRIM) commands when blocks are
    /// deleted. Should be enabled for SSDs.
    pub discard_blocks: bool,

    /// If true, disallow any mutations to the volume.
    pub read_only: bool,
}
