/*
    Implementation of a sequential log format greatly inspired by Google's RecordIO / LevelDB Log Formats

    Right now the binary format is basically equivalent to the LevelDB format but hopefully we will add compression to it as well


    - This is meant to be used for any application needing an append only log
    - It should be resilient to crashes such that records that were only partially
    - The general operations that this should support are:
        - Append new record to the end of the log
            - Also write a compressed record (or many tiny compressed records)
        - Read first record
        - Read last record
        - Find approximate record boundaries in a file and start a record read from that boundary
        - Iterate forwards or backwards from any readable record position


    References:
    - LevelDB's format is documented here
        - https://github.com/google/leveldb/blob/master/doc/log_format.md
    - RecordIO has been brief appearances in the file here:
        - See the Percolator/Caffeine papers
        - https://github.com/google/or-tools/blob/master/ortools/base/recordio.h
        - https://github.com/google/sling/blob/master/sling/file/recordio.h
        - https://github.com/google/trillian/blob/master/storage/tools/dump_tree/dumplib/dumplib.go
        - https://github.com/eclesh/recordio
        - https://news.ycombinator.com/item?id=16813030
        - https://github.com/google/riegeli

    TODO: Also 'ColumnIO' for columnar storage
*/

// TODO: Also useful would be to use fallocate on block sizes (or given a known
// maximum log size, we could utilize that size) At the least, we can perform
// heuristics to preallocate for the current append at the least

use std::path::Path;

use common::async_std::fs::{File, OpenOptions};
use common::async_std::io::prelude::{ReadExt, SeekExt, WriteExt};
use common::async_std::io::{Read, Seek, SeekFrom, Write};
use common::errors::*;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;

const BLOCK_SIZE: u64 = 32 * 1024;

/// Number of bytes needed to represent just the header of a single record (same
/// as in the LevelDB format)
const RECORD_HEADER_SIZE: u64 = 7;

enum_def!(RecordType u8 =>
    FULL = 1,
    FIRST = 2,
    MIDDLE = 3,
    LAST = 4
);

struct Record<'a> {
    checksum: u32,
    checksum_expected: u32,
    typ: RecordType,
    data: &'a [u8],
}

impl<'a> Record<'a> {
    /// Returns (parsed record, next offset in input)
    fn parse(input: &'a [u8]) -> Result<(Self, usize)> {
        if input.len() < (RECORD_HEADER_SIZE as usize) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Input shorter than record header").into());
        }

        let checksum = u32::from_le_bytes(*array_ref![input, 0, 4]);
        let length = u16::from_le_bytes(*array_ref![input, 4, 2]);
        let typ = RecordType::from_value(input[6])?;
        let data_start = RECORD_HEADER_SIZE as usize;
        let data_end = data_start + (length as usize);

        if input.len() < data_end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Input smaller than data length"
            ).into());
        }

        let data = &input[data_start..data_end];

        let checksum_expected = {
            let mut hasher = CRC32CHasher::new();
            // Hash [type, data]
            hasher.update(&input[6..data_end]);
            hasher.masked()
        };

        Ok((
            Self {
                checksum,
                checksum_expected,
                typ,
                data,
            },
            data_end,
        ))
    }
}

pub struct RecordReader {
    file: File,

    // file_size: u64,

    /// Current cursor into the file. This will be the offset at which the block
    /// buffer starts.
    ///
    /// At any point in time, we have parsed up to 'file_offset + block_offset'
    file_offset: u64,

    /// Buffer containing up to a single block. May be smaller than the BLOCK_SIZE
    /// if we hit the end of the file.
    block: Vec<u8>,

    /// Next offset in the block offset to be read
    block_offset: usize, /* TODO: Must know if at the end of the file to know if we can start
                          * writing (or consider the file offset to be
                          * at the start of the block ) */

                         /* TODO: Probably need to retain the last offset written without error
                          * to ensure that we can truncate when there
                          * is invalid data. */

                         /* TODO: Use a shared before when reading/writing */

                         /*	off: Option<u64>,
                          *	recs: Vec<Record<'a>>,
                          *	buf: Vec<u8> // [u8; BLOCK_SIZE] */

    /// Currently accumulated record data.
    /// If this is non-empty, then we have already received a FIRST record and we are
    /// currently waiting on MIDDLE|LAST records.
    output_buffer: Option<Vec<u8>>,
}

impl RecordReader {
    pub async fn open(path: &Path) -> Result<Self> {        
        let file = OpenOptions::new().read(true).open(path).await?;        

        let mut block = vec![];
        block.reserve_exact(BLOCK_SIZE as usize);

        Ok(Self {
            file,
            // file_size,
            file_offset: 0,
            block,
            block_offset: 0,
            output_buffer: None
        })
    }

    pub fn into_writer(self) -> RecordWriter {
        // TODO: Need to set O_APPEND
        RecordWriter { file: self.file }
    }

    // Generally should return the final position of the block
    // TODO: If we want to use this for error recovery, then it must be resilient to
    // not reading enough of the file (basically bounds check the length given
    // always (because even corruption in earlier blocks can have the same issue))
    // XXX: Also easier to verify the checksume right away

    /// Reads a complete block from the file starting at the given offset. After
    /// this is successful, the internal block buffer is use-able.
    async fn read_block(&mut self, off: u64) -> Result<()> {
        self.file_offset = off;
        self.block.clear();
        self.block_offset = 0;

        self.read_block_remainder().await?;
        Ok(())
    }

    /// Assuming that the current block 
    async fn read_block_remainder(&mut self) -> Result<()> {
        // At any point of time in this function this will be the number of bytes in the block
        // that we know are valid bytes read from the file. 
        let mut valid_length = self.block.len();

        if valid_length == BLOCK_SIZE as usize {
            return Ok(());
        }

        self.file.seek(SeekFrom::Start(self.file_offset + (valid_length as u64))).await?;

        self.block.resize(BLOCK_SIZE as usize, 0);

        // Read either until the end of the file or until we have filled the block.
        loop {
            let buf = &mut self.block[valid_length..];
            if buf.len() == 0 {
                break;
            }

            // NOTE: We assume that if there are actually 
            let n = self.file.read(buf).await.map_err(|e| {
                // On an error, reset the block back to its old state before the read so that we are
                // in a consistent state to retry later.
                self.block.truncate(valid_length);
                e
            })?;

            if n == 0 {
                break;
            }

            valid_length += n;
        }

        self.block.truncate(valid_length);
        Ok(())
    }

    // TODO: If there are multiple middle-blocks after each other in the same
    // block, we should error out.

    async fn read_record<'a>(&'a mut self) -> Result<Option<Record<'a>>> {
        // When there are < RECORD_HEADER_SIZE bytes remaining in the block, we know that they will
        // always be padding so we should advance to the next block.
        //
        // Otherwise we'll just ensure that the current block is fully read (at least up to the end of the file).
        if BLOCK_SIZE as usize - self.block_offset < RECORD_HEADER_SIZE as usize {
            self.read_block(self.file_offset + (self.block.len() as u64))
                .await?;
        } else {
            // NOTE: If we had previously hit the end of the file, then this will act to check if
            // more bytes have become available in the file.
            //
            // In the case of us not expecting any new bytes later on, then this may make reading
            // of a incomplete final block in the file very expensive as read() will be called
            // for every single record in the block.
            //
            // TODO: Consider only calling this if the block size is 0, or we just returned a None
            // in the previous call to read_record().
            self.read_block_remainder().await?;
        }

        let (record, record_size) = match Record::parse(&self.block[self.block_offset..]) {
            Ok(v) => v,
            Err(e) => {
                // If we didn't have enough bytes to parse the record, return None. Most likely the file
                // hasn't been flushed by a recent writer yet.
                if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                    if io_error.kind() == std::io::ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                }

                return Err(e);
            }
        };

        self.block_offset += record_size;

        if record.checksum_expected != record.checksum {
            return Err(err_msg("Checksum mismatch in record"));
        }

        Ok(Some(record))
    }

    /// Returns whether or not a complete record chain was read.
    async fn read_inner(&mut self, out: &mut Vec<u8>) -> Result<bool> {
        if out.len() == 0 {
            let first_record = match self.read_record().await? {
                Some(record) => record,
                None => {
                    return Ok(false);
                }
            };
    
            out.extend_from_slice(first_record.data);
            if first_record.typ == RecordType::FULL {
                return Ok(true);
            } else if first_record.typ == RecordType::FIRST {
                // Keep going
            } else {
                return Err(err_msg("Unexpected initial record type"));
            }
        }

        loop {
            let next_record = match self.read_record().await? {
                Some(record) => record,
                None => {
                    return Ok(false);
                }
            };

            out.extend_from_slice(next_record.data);

            match next_record.typ {
                RecordType::MIDDLE => {
                    continue;
                }
                RecordType::LAST => {
                    break;
                }
                _ => {
                    return Err(err_msg("Unexpected type in the middle of a user record"));
                }
            };
        }

        Ok(true)
    }

    /// NOTE: Values of Ok(None) can be retried later safely if the file grew in size.
    pub async fn read(&mut self) -> Result<Option<Vec<u8>>> {
        let mut out = self.output_buffer.take().unwrap_or_else(|| vec![]);

        // NOTE: If read_inner fails, we will end up dropping the 'out' buffer which will ensure that
        // we don't attempt to re-use it on subsequent read() calls.
        let is_complete = self.read_inner(&mut out).await?;

        if is_complete {
            Ok(Some(out))
        } else {
            self.output_buffer = Some(out);
            Ok(None)
        }
    }

    /// Call after you think you've fully read the entire file to verify that there are
    /// no more unread bytes after the current position (only use this if you know for sure
    /// that the file won't be modified in the near future).
    pub async fn check_eof(&self) -> Result<()> {
        let file_size = self.file.metadata().await?.len();
        if file_size != self.file_offset + (self.block_offset as u64) {
            return Err(err_msg("Unread bytes remaining at end of file"));
        }

        if let Some(data) = &self.output_buffer {
            if !data.is_empty() {
                return Err(err_msg("Incomplete record chain was read"));
            }
        }

        Ok(())
    }
}


pub struct RecordWriter {
    file: File,

    // NOTE: All the below are main


}

impl RecordWriter {
    pub async fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .await?;

        // let file_size = file.metadata().await?.len();


        Ok(Self {
            file,
        })
    }

    // pub async fn seek(&mut self, offset: u64) -> Result<()> {
    //     // TODO: This will require supporting reading a block which in the middle of it.
    // }

    /*
    pub fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().create_new(true).read(true).write(true).open(path)?;

        // Seek to the last block ofset
        // Read it and truncate to last complete block in it
        // We assume all previous blocks are still valid
        // If first record chain in last block is not terminated, must seek backwards



    }
    */

    // TODO: Support atomic writes across multiple unsynchronized writers?
    // TODO: Before writing, read the final block and verify that it is valid and not partially written. If it is, then
    // consider truncating or skipping one block ahead.
    // TODO: Verify this code
    // TODO: Buffer all writes and have a separate flush() operation (ideally we'd have the flushes on a timeout)
    pub async fn append(&mut self, data: &[u8]) -> Result<()> {
        let mut extent = self.file.seek(SeekFrom::End(0)).await?;

        // Must start in the next block if we can't fit at least a single
        // zero-length block in this block
        let rem = BLOCK_SIZE - (extent % BLOCK_SIZE);
        if rem < RECORD_HEADER_SIZE {
            extent += rem;
            self.file.set_len(extent).await?; // TODO: explicitly file with zeros instead.
            self.file.seek(SeekFrom::End(0)).await?;
        }

        let mut header = [0u8; RECORD_HEADER_SIZE as usize];

        let mut pos = 0;
        let mut first_record = true;
        while pos < data.len() {
            // TODO: Check for overflow although that should never happen if we did everything right.
            let rem = (BLOCK_SIZE - (extent % BLOCK_SIZE)) - RECORD_HEADER_SIZE;
            
            let take = std::cmp::min(rem as usize, data.len() - pos);

            // NOTE: It is insufficient to check that pos == 0 as we may have written a zero length packet
            // in the previous block.
            let typ = if first_record {
                if take == data.len() {
                    RecordType::FULL
                } else {
                    RecordType::FIRST
                }
            } else {
                if pos + take == data.len() {
                    RecordType::LAST
                } else {
                    RecordType::MIDDLE
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
            extent += (header.len() + take) as u64;
            first_record = false;
        }

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }
}
