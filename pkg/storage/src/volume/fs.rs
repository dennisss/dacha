use alloc::vec::Vec;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::prelude::FileExt;

use common::{enum_def, errors::*, InRange};
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::{GetHasherFactory, Hasher, HasherFactory};
use crypto::hkdf::HKDF;
use parsing::take_exact;
use protobuf::{Message, StaticMessage};
use uuid::UUID;

use crate::proto::volume::*;
use crate::volume::options::*;

use super::encryption::VolumeCipher;
use super::local::VolumeLocalFileTable;

/*
We should allow opening in a read only mode as well.
- This would do no recovery.
*/

/// Instance of a filesystem stored on a single disk partition.
pub struct VolumeFS {
    file: File,

    options: VolumeFSOpenOptions,

    header_config: VolumeHeaderConfig,

    cipher: Option<VolumeCipher>,

    local_files: VolumeLocalFileTable,
    // chunk_table
}

struct LogState {
    /// List of block indices which are corrupt.
    /// When this is not empty, it indicates that the volume needs repair before
    /// it can be used.
    corrupt_blocks: Vec<usize>,

    /// If true, it is possible that some information at the end of the log was
    /// lost due to corruption.
    corrupt_truncated: bool,

    /// Next offset relative to the start of the log at which we should write a
    /// block.
    next_offset: u64,

    next_sequence: u64,

    /// Starting position of the last entry in the log which contains
    /// a complete description of the snapshot file. We are not allowed to
    /// overwrite this position without moving the snapshot elsewhere in the
    /// log.
    snapshot_offset: u64,
}

impl VolumeFS {
    /// Identifier used to identify this file system in a GPT partition.
    pub const TYPE_GUID: UUID = uuid!("4bbd6499-3151-ffd3-792b-afb86455f44b");

    const MAGIC: &'static [u8] = b"daVM";

    /// Number of bytes reserved for storing the volume header at the beginning
    /// of the partition.
    const HEADER_BLOCK_SIZE: usize = 4096;

    const SNAPSHOT_PATH: &'static str = "/snapshot";

    /// Initializes a new filesystem
    pub fn create(mut file: File, options: VolumeFSOpenOptions) -> Result<Self> {
        // Make the header

        // Init the empty log (if not configured to go somewhere else)

        // Now open it. Assume that missing snapshot means no files

        todo!()
    }

    /// Opens an existing filesystem
    pub fn open(mut file: File, options: VolumeFSOpenOptions) -> Result<Self> {
        let mut header = {
            let mut buf = [0u8; Self::HEADER_BLOCK_SIZE];
            file.seek(SeekFrom::Start(options.relative_start))?;
            file.read_exact(&mut buf)?;

            let mut input = &buf[..];

            let magic = parse_next!(input, parsing::take_exact(Self::MAGIC.len()));
            if magic != Self::MAGIC {
                return Err(err_msg("Volume has incorrect magic"));
            }

            let checksum = parse_next!(input, parsing::binary::le_u32);
            let expected_checksum = {
                let mut hasher = CRC32CHasher::new();
                hasher.update(input);
                hasher.masked()
            };

            if checksum != expected_checksum {
                return Err(err_msg("Invalid volume header checksum"));
            }

            let size = parse_next!(input, parsing::binary::le_u16);
            let data = parse_next!(input, take_exact(size as usize));
            common::check_zero_padding(input)?;

            VolumeHeader::parse(data)?
        };

        let mut cipher = None;

        let raw_config = {
            if header.has_key_usage() {
                let key: &[u8] = options
                    .encryption_key
                    .as_ref()
                    .map(|v| v.as_ref())
                    .ok_or_else(|| {
                        err_msg("encryption_key must be provided to unlock encrypted volume")
                    })?;

                let c = cipher.insert(VolumeCipher::new(key, header.key_usage())?);
                c.decrypt(header.config_string(), &[])?
            } else {
                header.config_string().to_vec()
            }
        };

        let header_config = VolumeHeaderConfig::parse(&raw_config)?;

        if header_config.total_size() != options.size {
            return Err(err_msg("Volume size changed since initially created."));
        }

        if header_config.key_usage_copy().serialize()? != header.key_usage().serialize()? {
            return Err(err_msg("Inconsistent key usage settings defined in volume"));
        }

        let (log, latest_snapshot_idx) = {
            let mut batches = vec![];

            let mut latest_snapshot_idx = None;

            let mut buf = vec![];
            buf.resize(header_config.log_config().size() as usize, 0);
            file.read_exact_at(&mut buf, header_config.log_config().offset())?;

            if header_config.log_config().size() % header_config.log_config().record_size() != 0 {
                return Err(err_msg("Invalid log record size"));
            }

            for (block_i, mut block) in buf
                .chunks(header_config.log_config().record_size() as usize)
                .enumerate()
            {
                let checksum = parse_next!(block, parsing::binary::le_u32);

                let expected_checksum = {
                    let mut hasher = CRC32CHasher::new();
                    hasher.update(block);
                    hasher.masked()
                };

                if checksum != expected_checksum {
                    batches.push(None);
                    continue;
                }

                let data = match cipher.as_ref() {
                    Some(cipher) => {
                        cipher.decrypt(block, header_config.log_config().encryption_salt())?
                    }
                    None => block.to_vec(),
                };

                let size = u16::from_le_bytes(*array_ref![data, 0, 2]) as usize;
                if size > data.len() - 2 {
                    return Err(err_msg("Size too large"));
                }

                let batch = VolumeLogBatch::parse(&data[2..(size + 2)])?;

                for (i, entry) in batch.changes().iter().enumerate() {
                    if entry.change_file().file().path() == Self::SNAPSHOT_PATH {
                        latest_snapshot_idx = Some(core::cmp::max(
                            latest_snapshot_idx.unwrap_or((0, 0, 0)),
                            (batch.sequence_num(), block_i, i),
                        ));
                    }
                }

                batches.push(Some(batch));
            }

            (batches, latest_snapshot_idx)
        };

        // Read the latest snapshot
        let (_, latest_snapshot_batch, latest_snapshot_change) =
            latest_snapshot_idx.ok_or_else(|| err_msg("Volume has no latest snapshot"))?;

        // TODO: Ensure that snapshot blocks are never deleted until they are
        // overwritten in the log.
        let snapshot = {
            let change =
                &log[latest_snapshot_batch].as_ref().unwrap().changes()[latest_snapshot_change];
            let entry: &LocalFileEntry = change.change_file().file();
            let data = Self::read_local_file(entry, &header_config, cipher.as_ref(), &mut file)?;
            VolumeLocalSnapshot::parse(&data)?
        };

        let mut file_table = VolumeLocalFileTable::from_snapshot(&snapshot)?;

        /*
        Issue 1: Log written before data written
            => Have a write barrier before writing to the log.
        Issue 2: Log corruption causing rollback of the log.
            - This may lead us to referencing valid but incorrect data (e.g. a block was deleted and re-used by a different file).
            - Mitigations
                - Avoid writing to the logs of disks in the same replication pool at the same time
                - Require that deletions are fsynced to the log before the blocks can be re-used (would avoid simple power loss corruption issues)
                - Use reed solomon to compute parity data for all log entries (slow)
            - Partial corruption of a log is ok so long as we can completely trace forward a snapshot (cycle back to an entry with a lower sequence).
            - If a log entry is temporarily unavailable, we must re-write it before performing any operations so that we can avoid re-using sequence numbers if the entry re-appears.
                - Should also validate that all sequence numbers are sequential.
            - If a disk is on for a long time, we should proactively re-read the log and repair any errors.
            - Assuming append-only files, we can store the cumulative checksum of each file in the log.
            - Files must be re-validated on recovery.
                - all local files must be append-only
                - other files can be non-append only as they are implemented
                - Should also be wary of any disk read caches
            - If a log entry is corrupt, we should re-increment by at least that amount.
            - We can also add chunk ids as additional_data to encryption MACs so that unlikely that decryption will succeed



        If we see an old log, it is possible that blocks have been re-allocated since the snapshot.
        This implies that all snapshots referencing a block must be removed before a file can be removed.

        - Surviving log failure is important as power outages can mess up a write.

        */

        // Seek to the entry immediately after the snapshot.
        // When the log is not corrupt, there should always be at least one that records
        // the new snapshot location.
        let mut next_log_i = {
            let mut found_i = None;
            for (i, batch) in log.iter().enumerate() {
                if let Some(batch) = batch {
                    if batch.sequence_num() == snapshot.sequence_num() + 1 {
                        found_i = Some(i);
                        break;
                    }
                }
            }

            found_i
                .ok_or_else(|| err_msg("Failed to find a log entry after the latest snapshot"))?
        };

        loop {
            let next_batch = log[next_log_i].as_ref().unwrap();
            file_table.apply(next_batch)?;

            let next_i = (next_log_i + 1) % log.len();

            if let Some(next_next_batch) = &log[next_i] {
                if next_next_batch.sequence_num() < next_batch.sequence_num() {
                    break;
                }

                next_log_i = next_i;
            } else {
                // TODO: Implement recovery by checking all files in this case.
                return Err(err_msg("Log tail is corrupted"));
            }
        }

        /*
        Other things to do:
        - read the /.volume/config
        - initialize all pools
            ^ Pool initialization is a higher level decision

        - Clean up any invalid log entries.
        - Loop through all the [local files, chunks] to get the allocated blocks list
        */

        /*
        For local files,
        - 4KB blocks with checksums
            - 4 byte overhead per block

        - 16KB encrypted segments
            - AES-GCM: 12 nonce + 16 MAC =
        - 44 bytes of overhead (0.27%)
        - Ideal 'block size' is 16340
            - So for log files, we should tune

        So reading a local file:
        - Read all data
        - Verify each block checksum

        - Encrypt the checksum?
            - THis means that we need to replicate separate encrpytion parts
        - When reading, we always need to read an encrypted segment.
            - So checksumming below once a segment won't help with read performance as we always need to validate the MAC.
        - Other challenges:
            -
        -

        - TODO: Use AES-GCM-SIV and maybe add the chunk ids as additional data?

        - Appending to a file with the last segment partially complete
            - Assuming the client has cached the checksum and encryption state of the last block, then we can just write the next block to a new position (should be the normal case)
            - Otherwise, need to re-read the last segment and
        */

        /*
        - With RS=4.2, need 4 data blocks before we can write parity
        - When we perform encryption, we similarly want to only write complete
        - How to write < 1 stride?


        Encryption improvements:
        - More efficient to encrypt more data
        - Should we support partial writes?
        - We want all blocks to be full.
            - Not writing a full block count will force a re-sync.

        -

        Decisions:
        - Every disk

        - We require a special key in

        - The log should pretty much always have a single key

        How to encrypt:
        - Need to have a
        */

        // Read all sectors of the log (should )

        /*

        */

        // read

        todo!()
    }

    // Reads an entire local file to a memory buffer.
    fn read_local_file(
        entry: &LocalFileEntry,
        config: &VolumeHeaderConfig,
        cipher: Option<&VolumeCipher>,
        file: &mut File,
    ) -> Result<Vec<u8>> {
        let mut data = vec![];

        for extent in entry.extents() {
            let mut extent_data = vec![];
            extent_data.resize(
                ((extent.end_block() - extent.start_block()) * config.block_size()) as usize,
                0,
            );

            file.read_exact_at(
                &mut extent_data,
                (extent.start_block() * config.block_size()),
            )?;

            for block in extent_data.chunks(config.local_file_encoding().record_size() as usize) {
                let checksum = u32::from_le_bytes(*array_ref![block, 0, 4]);
                let block_data = &block[4..];

                // TODO: Also validate the full file checksum.
                let expected_checksum = {
                    let mut hasher = CRC32CHasher::new();
                    hasher.update(block_data);
                    hasher.masked()
                };

                if expected_checksum != checksum {
                    return Err(err_msg("Block checksum mismatch"));
                }

                data.extend_from_slice(block_data);
            }
        }

        /*
        Can different extents be combined with the same

        An encrypted segment may have 4 blocks
        - A user may initially write one with 4 blocks and then re-use the first 3 blocks for appending to the 4th block.
        - Padding should normally be small so always pad up when encrypting

        TODO: If we ever need to move data with a different block size, how we do preserve these semantics?

        - We will reserve salts chunk by chunk, so we for now won't allow changing it in the middle of a file.

        */

        if let Some(cipher) = cipher {
            let mut new_data = vec![];

            for encrypted_block in data.chunks(config.local_file_encoding().segment_size() as usize)
            {
                let plaintext = cipher.decrypt(encrypted_block, entry.encryption_salt())?;
                new_data.extend_from_slice(&plaintext);
            }

            data = new_data;
        }

        Ok(data)
    }
}
