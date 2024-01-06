//! Implementation of a sequential log format mostly compatible with
//! LevelDB/RocksDB.
//!
//! See the format documentation here:
//! https://github.com/google/leveldb/blob/master/doc/log_format.md

use common::errors::*;
use common::io::{Readable, Writeable};
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use file::sync::{SyncedFile, SyncedPath};
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

const BLOCK_SIZE: u64 = 32 * 1024;

/// Number of bytes needed to represent just the header of a single fragment.
const FRAGMENT_HEADER_SIZE: u64 = 7;

enum_def!(FragmentType u8 =>
    ZERO = 0,
    FULL = 1,
    FIRST = 2,
    MIDDLE = 3,
    LAST = 4
);

struct Fragment<'a> {
    typ: FragmentType,
    data: &'a [u8],
}

impl<'a> Fragment<'a> {
    /// Parses the next fragment from the given input.
    ///
    /// Arguments:
    /// - input: All data available for the current block.
    /// - remaining_block_length: Including the length of 'input', the remaining
    ///   bytes until the next block.
    ///
    /// Returns (parsed fragment, next offset in input)
    fn parse(
        input: &'a [u8],
        remaining_block_length: usize,
    ) -> Result<(Self, usize), RecordReadError> {
        if remaining_block_length < (FRAGMENT_HEADER_SIZE as usize) {
            return Err(RecordReadError::Corrupt);
        } else if input.len() == 0 {
            return Err(RecordReadError::EndOfFile);
        } else if input.len() < (FRAGMENT_HEADER_SIZE as usize) {
            return Err(RecordReadError::Incomplete);
        }

        let checksum = u32::from_le_bytes(*array_ref![input, 0, 4]);
        let length = u16::from_le_bytes(*array_ref![input, 4, 2]);
        let typ = FragmentType::from_value(input[6]).map_err(|_| RecordReadError::Corrupt)?;
        let data_start = FRAGMENT_HEADER_SIZE as usize;
        let data_end = data_start + (length as usize);

        if typ == FragmentType::ZERO {
            // Zero type records take up the remainder of the block.

            // Ensure we have the entire block (otherwise we may attempt to read more
            // fragments after the ZERO type fragment on the next call to Fragment::parse).
            if remaining_block_length != input.len() {
                return Err(RecordReadError::Incomplete);
            }

            let all_zero = common::check_zero_padding(input).is_ok();
            if all_zero {
                return Ok((Fragment { typ, data: &[] }, input.len()));
            } else {
                return Err(RecordReadError::Corrupt);
            }
        }

        if remaining_block_length < data_end {
            return Err(RecordReadError::Corrupt);
        } else if input.len() < data_end {
            return Err(RecordReadError::Incomplete);
        }

        let data = &input[data_start..data_end];

        let checksum_expected = {
            let mut hasher = CRC32CHasher::new();
            // Hash [type, data]
            hasher.update(&input[6..data_end]);
            hasher.masked()
        };

        if checksum != checksum_expected {
            return Err(RecordReadError::Corrupt);
        }

        Ok((Self { typ, data }, data_end))
    }
}

/// Reader for sequentially processing the records stored in a log file
/// previously created with the RecordWriter.
///
/// An instance of this can exist with a concurrent RecordWriter so long as the
/// RecordWriter only performs end appends and was not opened with
/// allow_truncation.
pub struct RecordReader {
    path: LocalPathBuf,

    recovery_mode: RecordRecoveryMode,

    file: LocalFile,

    /// Current cursor into the file. This will be the offset at which the block
    /// buffer starts.
    ///
    /// At any point in time, we have parsed up to 'file_offset + block_offset'
    file_offset: u64,

    /// Buffer containing up to a single block. May be smaller than the
    /// BLOCK_SIZE if we hit the end of the file.
    block: Vec<u8>,

    /// Next offset in the block offset to be read
    block_offset: usize,

    /// The corrupt block buffer contains corrupt data so will be skipped in the
    /// next read.
    block_corrupt: bool,

    /// Currently accumulated record data if we hit the EOF or a Dropped error
    /// before finishing the last record.
    ///
    /// This is a tuple of (start_file_offset, accumalated_record_data) where:
    /// - 'start_file_offset' is the
    /// - 'accumalated_record_data' is data from at least the FIRST fragment and
    ///   zero or more sequential MIDDLE fragments.
    ///
    /// If not None, this will be used as the starting point for future reads so
    /// that we never need to re-read blocks of a file.
    output_buffer: Option<RecordBuffer>,
}

/// Failure reason returned while trying to read the next record from a log
/// file.
#[error]
pub enum RecordReadError {
    /// We are at the end of the file. No partial reads or failures occured.
    ///
    /// Future reads will repeatedly return this result until more data is
    /// written to the file.
    EndOfFile,

    /// We read one or more complete records from the file but they were corrupt
    /// (checksum mismatch).
    ///
    /// Future reads will attempt to skip over the corrupted section until
    /// another result is returned.
    Corrupt,

    /// While reading a fragmented record, another one was started so we dropped
    /// the data associated with the in-complete record.
    Dropped,

    /// We hit the end of the file before reading a complete record.
    ///
    /// NOTE: Unless the underlying filesystem guarantees read/write integrity,
    /// we can't differentiate in all cases between an incomplete file and a
    /// file ending in a corrupted record.
    ///
    /// Future reads will repeatedly return this result until more data is
    /// written to the file.
    Incomplete,

    /// The underlying filesystem returned an unknown error while we were trying
    /// to read from it.
    IoFailure(Error),
}

/// Filter to use when reading records from the log.
pub enum RecordRecoveryMode {
    /// Disallows any fragments in the log file to have an
    /// inconsistent/incomplete format.
    Strict,

    /// Allows the end of the file to end in an incomplete record.
    ///
    /// This allows a reader to open logs from writers that weren't gracefully
    /// flushed, but this is only safe if the filesystem maintains its own
    /// integrity checks for written data.
    ///
    /// [Default]
    AllowIncomplete,
    /*
    /// In addition to what is allowed by AllowIncomplete, we will ignore
    /// dropping of valid incomplete records that are being superceded by newer
    /// writes.
    AllowDropped,
    */
}

struct RecordBuffer {
    /// Whether or not we got a FIRST|FULL fragment yet.
    started: bool,

    /// Whether or not we got a LAST|FULL fragment yet.
    ended: bool,

    /// Position in the file where the FIRST|FULL fragment for this record is
    /// located.
    start_file_offset: u64,

    /// All the data accumulated across all sequential fragments since the last
    /// FIRST|FULL fragment.
    data: Vec<u8>,
}

impl RecordReader {
    /// Opens an existing log file for reading.
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // TODO: Open with O_DIRECT
        let file = LocalFile::open(path)?;

        let mut block = vec![];
        block.reserve_exact(BLOCK_SIZE as usize);

        Ok(Self {
            path: path.to_owned(),
            recovery_mode: RecordRecoveryMode::AllowIncomplete,
            file,
            file_offset: 0,
            block,
            block_offset: 0,
            block_corrupt: false,
            output_buffer: None,
        })
    }

    pub fn set_recovery_mode(&mut self, mode: RecordRecoveryMode) {
        self.recovery_mode = mode;
    }

    /// Reads the next complete record from the file.
    ///
    /// Will return None once we are out of data to read. This will also filter
    /// out any inconsistencies.
    ///
    /// - Will error out if we detect corruption.
    /// - Will return None if we hit the end of file (or the file ends in an
    ///   incompletely written record).
    pub async fn read(&mut self) -> Result<Option<Vec<u8>>> {
        match self.read_raw().await {
            Ok(v) => Ok(Some(v)),
            Err(RecordReadError::EndOfFile) => Ok(None),
            Err(e @ RecordReadError::Incomplete) => match self.recovery_mode {
                RecordRecoveryMode::Strict => Err(e.into()),
                RecordRecoveryMode::AllowIncomplete => Ok(None),
            },
            Err(e) => Err(e.into()),
        }
    }

    /// Attempts to read the next record from the file.
    ///
    /// This does not perform any filtering and will directly return any
    /// failures seen in the log file. This will normally return
    /// RecordReadError::EndOfFile if the log was gracefully written and
    /// flushed.
    pub async fn read_raw(&mut self) -> Result<Vec<u8>, RecordReadError> {
        let mut out = self.output_buffer.take().unwrap_or_else(|| RecordBuffer {
            started: false,
            ended: false,
            start_file_offset: self.file_offset + (self.block_offset as u64),
            data: vec![],
        });

        // NOTE: If read_inner fails, we will end up dropping the 'out' buffer which
        // will ensure that we don't attempt to re-use it on subsequent read()
        // calls.
        match self.read_inner(&mut out).await {
            Ok(()) => Ok(out.data),
            Err(e @ RecordReadError::Corrupt) => {
                drop(out);
                Err(e)
            }
            Err(mut e) => {
                // If we hit the EOF while reading a single fragment, we need to factor in the
                // other all state of our buffer across multiple fragments.
                if let RecordReadError::EndOfFile = e {
                    if out.started {
                        e = RecordReadError::Incomplete;
                    }
                }

                self.output_buffer = Some(out);
                Err(e)
            }
        }
    }

    /// Returns whether or not a complete record chain was read.
    async fn read_inner(&mut self, out: &mut RecordBuffer) -> Result<(), RecordReadError> {
        while !out.ended {
            let fragment = self.read_fragment().await?;

            match fragment.typ {
                FragmentType::ZERO => {
                    // Allow a block to end in all zeros. This is mainly for LevelDB compatibility
                    // and is ok so long as the zeros aren't in the middle of a fragment.
                    if out.started {
                        return Err(RecordReadError::Corrupt);
                    }
                }
                FragmentType::FULL => {
                    let dropping = out.started;

                    out.data.extend_from_slice(fragment.data);
                    out.started = true;
                    out.ended = true;

                    if dropping {
                        return Err(RecordReadError::Dropped);
                    }
                }
                FragmentType::FIRST => {
                    let dropping = out.started;

                    out.data.extend_from_slice(fragment.data);
                    out.started = true;

                    if dropping {
                        return Err(RecordReadError::Dropped);
                    }
                }
                FragmentType::MIDDLE => {
                    // TODO: If there are multiple MIDDLE fragments after each other in the same
                    // block, we should error out.

                    if !out.started {
                        return Err(RecordReadError::Corrupt);
                    }

                    out.data.extend_from_slice(fragment.data);
                }
                FragmentType::LAST => {
                    if !out.started {
                        return Err(RecordReadError::Corrupt);
                    }

                    out.data.extend_from_slice(fragment.data);
                    out.ended = true;
                }
            }
        }

        Ok(())
    }

    /// Attempts to read the next fragment from the file.
    async fn read_fragment<'a>(&'a mut self) -> Result<Fragment<'a>, RecordReadError> {
        if self.block_corrupt {
            self.file_offset += BLOCK_SIZE;
            self.block_offset = 0;
            self.block.clear();
        }

        // Try again to read the rest of the current block which may not be full if we
        // previously hit the end of the file (or the last one was correupted).
        self.read_block_remainder().await?;

        // If the remainder of the block can't fit a fragment header, then it must be
        // filled with zeros (or end earlier at the EOF).
        if BLOCK_SIZE as usize - self.block_offset < FRAGMENT_HEADER_SIZE as usize {
            let remaining = &self.block[self.block_offset..];

            common::check_zero_padding(remaining).map_err(|_| {
                self.block_corrupt = true;
                RecordReadError::Corrupt
            })?;

            if self.block.len() == BLOCK_SIZE as usize {
                self.read_block(self.file_offset + BLOCK_SIZE).await?;
            } else {
                // We hit the end of the file before we read the rest of the padding in the
                // current block.
                return Err(RecordReadError::EndOfFile);
            }
        }

        let remaining_block_length = (BLOCK_SIZE as usize) - self.block_offset;

        match Fragment::parse(&self.block[self.block_offset..], remaining_block_length) {
            Ok((fragment, fragment_size)) => {
                self.block_offset += fragment_size;
                Ok(fragment)
            }
            Err(e) => {
                if let RecordReadError::Corrupt = e {
                    self.block_corrupt = true;
                }

                Err(e)
            }
        }
    }

    /// Reads up to a complete block from the file starting at the given file
    /// offset. After this is successful, the internal block buffer is
    /// use-able.
    async fn read_block(&mut self, off: u64) -> Result<(), RecordReadError> {
        self.file_offset = off;
        self.block.clear();
        self.block_offset = 0;

        self.read_block_remainder().await?;
        Ok(())
    }

    /// While the current block buffer (self.buffer) contains less than
    /// BLOCK_SIZE number of bytes and we haven't hit the end of the file, this
    /// will read more bytes into the buffer.
    async fn read_block_remainder(&mut self) -> Result<(), RecordReadError> {
        // At any point of time in this function this will be the number of bytes in the
        // block that we know are valid bytes read from the file.
        let mut valid_length = self.block.len();

        if valid_length == BLOCK_SIZE as usize {
            return Ok(());
        }

        // TODO: Reads should be block aligned if we are using O_DIRECT.
        self.file.seek(self.file_offset + (valid_length as u64));

        self.block.resize(BLOCK_SIZE as usize, 0);

        // Read either until the end of the file or until we have filled the block.
        while valid_length < self.block.len() {
            let buf = &mut self.block[valid_length..];

            let n = self.file.read(buf).await.map_err(|e| {
                // On an error, reset the block back to its old state before the read so that we
                // are in a consistent state to retry later.
                self.block.truncate(valid_length);

                RecordReadError::IoFailure(e)
            })?;

            if n == 0 {
                break;
            }

            valid_length += n;
        }

        self.block.truncate(valid_length);
        Ok(())
    }

    /// Creates a writer for appending to the end of this log.
    ///
    /// The writer will be compatible with the currently set reader recovery
    /// mode.
    ///
    /// - The reader must have been already read until the end of the log.
    /// - If the file contained any corruption, it will be preserved and we will
    ///   simply continue appending at a recoverable position (next full block
    ///   offset) after it.
    /// - Because we strictly append to the log, it may be impossible to safely
    ///   open the log for appending without truncation. In these cases, None
    ///   will be returned.
    ///
    /// Notes on implementation:
    /// - The most important factor when appending to a log file is ensuring
    ///   that we do not alter the values of previously written records.
    /// - If the file ends with an incomplete record, we could truncate it to
    ///   right before the record to continue.
    ///     - If the reader already acknowledged that having an 'incomplete'
    ///       file is ok, then this should generally be safe as we are not
    ///       removing any value which was previously provided to a reader.
    ///     - The main issue with this is that because our reader implementation
    ///       only looks forward, truncation would break readers or cause them
    ///       to see invalid data.
    ///     - If we do allow truncation, then this function will never return
    ///       None.
    ///     - Even if truncation is enabled, we will attempt to avoid it if a
    ///       better solution is available.
    ///     - If we can guarantee with complete certainty that the writer
    ///       doesn't isn't sensitive to the value of the last record in the
    ///       log, we could allow this.
    /// - If the file ends in a complete fragment, but not a complete record, we
    ///   can directly append to the end of the file, but only if readers are
    ///   tolerant to dropped entries.
    /// - If the file ends in an incomplete fragment, we will NOT pad up to the
    ///   next block offset.
    ///     - If it is very realistic that a record was filled with mostly zeros
    ///       so padding an incomplete record may make it into a valid record.
    ///       Because the creator of the writer is not aware of the existence of
    ///       that entry, this may lead to ghost appends.
    pub async fn into_writer(mut self, allow_truncation: bool) -> Result<Option<RecordWriter>> {
        let final_error = match self.read_raw().await {
            Err(e @ RecordReadError::EndOfFile | e @ RecordReadError::Incomplete) => e,
            _ => {
                return Err(err_msg(
                    "Expected the RecordReader to be at the end of the file before writing.",
                ))
            }
        };

        // Re-open for writes.
        // TODO: Use O_DIRECT.
        let mut file =
            LocalFile::open_with_options(&self.path, LocalFileOpenOptions::new().write(true))?;

        let current_length = file.metadata().await?.len();

        // Position at which the writer will be appending new records.
        let mut append_offset = self.file_offset + (self.block_offset as u64);

        if let Some(record) = self.output_buffer {
            append_offset = record.start_file_offset;
        }

        if !allow_truncation {
            if append_offset < current_length {
                return Ok(None);
            }
        }

        // This might pad the file with zeros up to the next block if the last block in
        // the file contains corruption.
        file.set_len(append_offset).await?;

        file.seek(append_offset);

        let file = LogWriter::create(LogWriterOptions::default(), file).await?;

        Ok(Some(RecordWriter {
            file,
            extent: append_offset,
        }))
    }
}

pub struct RecordWriter<Output: Writeable = LogWriter> {
    /// NOTE: If this is a file, it should already be seeked to the end of the
    /// file.
    file: Output,

    /// Byte offset at which we will next write in the file.
    extent: u64,
}

impl RecordWriter<LogWriter> {
    /// Creates a new record log file at the given path (it must not already
    /// exist).
    pub async fn create_new<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let mut file = LocalFile::open_with_options(
            path,
            LocalFileOpenOptions::new()
                .write(true)
                .create(true)
                .sync_on_flush(true)
                .create_new(true),
        )?;

        file.seek(0);

        let file = LogWriter::create(LogWriterOptions::default(), file).await?;

        Ok(Self { file, extent: 0 })
    }

    /// Opens an existing log file with the write cursor places after the last
    /// valid record in the file.
    ///
    /// This is similar to RecordReader::into_writer but can more cheaply append
    /// to the end of a file which we don't care about reading.
    ///
    /// NOTE: It is strongly recommended to instead read back the start of the
    /// file and convert that to a writer once everything has been read.
    pub async fn open_existing(path: &LocalPath) -> Result<Option<Self>> {
        let mut file = LocalFile::open_with_options(path, LocalFileOpenOptions::new().write(true))?;

        todo!()
    }

    pub fn path(&self) -> &LocalPath {
        self.file.path()
    }

    pub fn new_flush_subscriber(&self) -> LogFlushSubscriber {
        self.file.new_flush_subscriber()
    }
}

impl<Output: Writeable> RecordWriter<Output> {
    /// NOTE: The writer MUST be empty.
    pub fn new(writer: Output) -> Self {
        Self {
            file: writer,
            extent: 0,
        }
    }

    /// Size of the file if we were to flush all previously appended changes.
    pub fn current_size(&self) -> u64 {
        self.extent
    }

    /// TODO: Buffer all writes and have a separate flush() operation (ideally
    /// we'd have the flushes on a timeout)
    ///
    /// TODO: Ensure this future is never dropped.
    ///
    /// TODO: Implement pre-allocation with fallocate
    pub async fn append(&mut self, data: &[u8]) -> Result<()> {
        // Must start in the next block if we can't fit at least a single
        // zero-length block in this block
        let rem = BLOCK_SIZE - (self.extent % BLOCK_SIZE);
        if rem < FRAGMENT_HEADER_SIZE {
            self.extent += rem;
            self.file.write_all(common::zeros(rem as usize)).await?;
        }

        let mut header = [0u8; FRAGMENT_HEADER_SIZE as usize];

        let mut pos = 0;
        let mut first_record = true;
        while pos < data.len() || first_record {
            let rem = (BLOCK_SIZE - (self.extent % BLOCK_SIZE)) - FRAGMENT_HEADER_SIZE;

            let take = std::cmp::min(rem as usize, data.len() - pos);

            // NOTE: It is insufficient to check that pos == 0 as we may have written a zero
            // length packet in the previous block.
            let typ = if first_record {
                if take == data.len() {
                    FragmentType::FULL
                } else {
                    FragmentType::FIRST
                }
            } else {
                if pos + take == data.len() {
                    FragmentType::LAST
                } else {
                    FragmentType::MIDDLE
                }
            } as u8;

            let data_slice = &data[pos..(pos + take)];

            // Checksum of [ type, data ]
            let sum = {
                let mut hasher = CRC32CHasher::new();
                hasher.update(&[typ]);
                hasher.update(data_slice);
                hasher.masked()
            };

            header[0..4].copy_from_slice(&sum.to_le_bytes());
            header[4..6].copy_from_slice(&(take as u16).to_le_bytes());
            header[6] = typ;

            self.file.write_all(&header).await?;
            self.file.write_all(data_slice).await?;
            pos += take;
            self.extent += (header.len() + take) as u64;
            first_record = false;
        }

        Ok(())
    }

    /// TODO: Ensure this future is never dropped.
    ///
    /// TODO: Ensure that we always run this on program cleanup (to ensure that
    /// minimally everything is out of out memory and in the OS)
    ///
    /// TODO: Eventually this may become more complex so we will need to be more
    /// complex testing of flushing at different points in the writer.
    pub async fn flush(&mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crypto::random::Rng;

    use super::*;

    // TODO: We should automatically save all of these files as goldens which must
    // align with older versions of the files.

    #[testcase]
    async fn generally_works() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut writer = RecordWriter::create_new(&log_path).await?;
        writer.append(b"hello").await?;
        writer.append(b"world").await?;
        writer.flush().await?;
        drop(writer);

        let mut reader = RecordReader::open(&log_path).await?;

        assert_eq!(reader.read().await?, Some(b"hello".to_vec()));
        assert_eq!(reader.read().await?, Some(b"world".to_vec()));
        assert_eq!(reader.read().await?, None);

        Ok(())
    }

    #[testcase]
    async fn insert_a_really_large_record() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut data = vec![0u8; 100_000];
        crypto::random::clocked_rng().generate_bytes(&mut data);

        let mut writer = RecordWriter::create_new(&log_path).await?;
        writer.append(&data).await?;
        writer.flush().await?;
        drop(writer);

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, Some(data));
        assert_eq!(reader.read().await?, None);

        Ok(())
    }

    #[testcase]
    async fn an_empty_file_is_a_valid_log() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;

        let log_path = temp_dir.path().join("log");
        file::write(&log_path, "").await?;

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, None);

        let mut writer = reader.into_writer(false).await?.unwrap();
        assert_eq!(writer.current_size(), 0);

        writer.append(b"hi!").await?;

        writer.flush().await?;
        drop(writer);

        let mut data = file::read(&log_path).await?;

        // One FULL record.
        assert_eq!(&data[..], b"\x35\x9a\x49\x51\x03\x00\x01hi!");

        Ok(())
    }

    #[testcase]
    async fn block_sized_fragments() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;

        let log_path = temp_dir.path().join("log");

        let mut writer = RecordWriter::create_new(&log_path).await?;

        let record1 = vec![0x10u8; (BLOCK_SIZE - FRAGMENT_HEADER_SIZE) as usize];
        writer.append(&record1[..]).await?;
        assert_eq!(writer.current_size(), BLOCK_SIZE);

        // A second record still fits perfectly into 2 blocks.
        let mut record2 = vec![0x20u8; (BLOCK_SIZE - FRAGMENT_HEADER_SIZE) as usize];
        writer.append(&record2[..]).await?;
        assert_eq!(writer.current_size(), 2 * BLOCK_SIZE);

        // Adding a small entry to misalign stuff
        let mut record3 = vec![0x30u8; 10];
        writer.append(&record3[..]).await?;
        assert_eq!(
            writer.current_size(),
            2 * BLOCK_SIZE + (FRAGMENT_HEADER_SIZE + 10)
        );

        // Now a full block record must be split.
        let mut record4 = vec![0x40u8; (BLOCK_SIZE - FRAGMENT_HEADER_SIZE) as usize];
        writer.append(&record4[..]).await?;
        assert_eq!(
            writer.current_size(),
            2 * BLOCK_SIZE + (FRAGMENT_HEADER_SIZE + 10) + (BLOCK_SIZE + FRAGMENT_HEADER_SIZE)
        );

        writer.flush().await?;
        drop(writer);

        let mut data = file::read(&log_path).await?;

        assert_eq!(data[6], FragmentType::FULL as u8);
        assert_eq!(&data[7..(BLOCK_SIZE as usize)], &record1);

        assert_eq!(data[(BLOCK_SIZE as usize) + 6], FragmentType::FULL as u8);
        assert_eq!(
            &data[(BLOCK_SIZE as usize) + 7..(2 * BLOCK_SIZE as usize)],
            &record2
        );

        assert_eq!(
            data[(2 * BLOCK_SIZE as usize) + 6],
            FragmentType::FULL as u8
        );
        assert_eq!(
            &data[(2 * BLOCK_SIZE as usize) + 7..(2 * BLOCK_SIZE as usize) + 17],
            &record3
        );

        assert_eq!(
            data[(2 * BLOCK_SIZE as usize) + 23],
            FragmentType::FIRST as u8
        );

        assert_eq!(
            data[(3 * BLOCK_SIZE as usize) + 6],
            FragmentType::LAST as u8
        );

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, Some(record1));
        assert_eq!(reader.read().await?, Some(record2));
        assert_eq!(reader.read().await?, Some(record3));
        assert_eq!(reader.read().await?, Some(record4));
        assert_eq!(reader.read().await?, None);

        Ok(())
    }

    #[testcase]
    async fn generally_supports_zero_sized_records() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut writer = RecordWriter::create_new(&log_path).await?;

        writer.append(b"").await?;
        writer.append(b"hello").await?;
        writer.append(b"").await?;
        writer.append(b"").await?;
        writer.append(b"world").await?;
        writer.append(b"").await?;
        writer.flush().await?;

        drop(writer);

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, Some(b"".to_vec()));
        assert_eq!(reader.read().await?, Some(b"hello".to_vec()));
        assert_eq!(reader.read().await?, Some(b"".to_vec()));
        assert_eq!(reader.read().await?, Some(b"".to_vec()));
        assert_eq!(reader.read().await?, Some(b"world".to_vec()));
        assert_eq!(reader.read().await?, Some(b"".to_vec()));
        assert_eq!(reader.read().await?, None);

        Ok(())
    }

    #[testcase]
    async fn supports_truncated_end_fragment() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut writer = RecordWriter::create_new(&log_path).await?;
        writer.append(b"hello").await?; // 12 byte FULL fragment
        writer.append(b"world").await?; // 12 byte FULL fragment
        writer.flush().await?;
        drop(writer);

        {
            let stat = file::metadata(&log_path).await?;
            assert_eq!(stat.len(), 24);
        }

        {
            let mut file =
                LocalFile::open_with_options(&log_path, LocalFileOpenOptions::new().write(true))?;
            file.set_len(22).await?;
        }

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, Some(b"hello".to_vec()));
        assert_eq!(reader.read().await?, None);

        let mut writer = reader.into_writer(true).await?.unwrap();

        // Rolls back the last incomplete record
        {
            let stat = file::metadata(&log_path).await?;
            assert_eq!(stat.len(), 12);
        }

        writer.append(b"cat").await?;
        writer.flush().await?;
        drop(writer);

        {
            let stat = file::metadata(&log_path).await?;
            assert_eq!(stat.len(), 22);
        }

        let mut reader = RecordReader::open(&log_path).await?;
        assert_eq!(reader.read().await?, Some(b"hello".to_vec()));
        assert_eq!(reader.read().await?, Some(b"cat".to_vec()));
        assert_eq!(reader.read().await?, None);

        Ok(())
    }

    /*
    // #[testcase]
    async fn supports_truncated_end_fragment_large() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut writer = RecordWriter::create_new(&log_path).await?;
        writer.append(b"hello").await?; // 12 byte FULL fragment
        writer.append(b"world").await?; // 12 byte FULL fragment
        writer.flush().await?;
        drop(writer);

        {
            let stat = file::metadata(&log_path).await?;
            assert_eq!(stat.len(), 24);
        }

        Ok(())
    }
    */

    // Test writing when the amount of bytes left in the current block is small

    // Check handling of zero padded blocks.

    // Most important test is to check for first/middle/last support

    // Then test that we can use into_writer with existing logs in various
    // states.

    /*
    #[testcase]
    async fn seek_past_corruption() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        //

        Ok(())
    }
    */

    // let mut hasher = CRC32CHasher::new();
    // hasher.update(&expected[6..]);
    // println!("{:0x?}", hasher.masked().to_le_bytes());

    // println!("{:0x?}", &data[..]);

    /*
    - Test disallow non-zero padding

    - Test disallow zeros at the beginning of a block if

    - Test that if we have a file ending in zeros, we must extend it to a full block before appending

    - Test we can have a full zero length record at the end of a block
    - Test that if we don't have enough space for a header, we zero pad and skip to the next block when appending.

    If a block is completely zero, we should skip it (even if that is the first block).

    If a record is larger than one block, it must span multiple blocks
    - Test with it starting at the start of a block or un the middle of one
    - Test with 1 FIRST + 1 LAST
    - Test eith 1 FIRST + 2 MIDDLE + 1 LAST

    - Test if the file is truncated to either:
        - A complete fragment or an incomplete one, then we can not notice the final chunk.

    - Test with different invalid length values to ensure we don't panic if it is larger than the BLOCK_SIZE or remaining_block_Size

    - Test with a corrupt fragment type
    - Test with a corrupt checksum getting rejected

    - Test into_writer
        - Also with an empty file
        - Refuse to truncate a partially written file (unless truncation enabled)
        - Pad up to next record if there is corruption.
        - Pad up to next record if ending in zeros.

    - Test reading and skipping over dropped records.

    - Corruption test.
        - Can skip a FIRST|MIDDLE entry without a LAST
        - Can skip a corrupt record but seeking to the next block (ignore any other valid segments in the block)


    If a

    TODO: If we ever do a pread/write, we can use ropes/cords
    */
}
