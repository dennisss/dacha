use alloc::vec::Vec;
use core::ops::Deref;
use crypto::checksum::crc16::crc16_incremental_lut;

use common::collections::FixedVec;
use common::errors::*;
use crypto::hasher::Hasher;
use executor::mutex::Mutex;

use crate::eeprom::EEPROM;

// 4 bytes for write_count
// 2 bytes for length
// 2 bytes for CRC-16
const BLOCK_OVERHEAD: usize = 4 + 2 + 2;

#[derive(Clone, Copy, Debug, Errable)]
#[repr(u32)]
pub enum BlockOpenError {
    OutOfSpace,
    ExistingFileTooSmall,
}

pub struct BlockStorage {
    state: Mutex<BlockStorageState>,
}

struct BlockStorageState {
    eeprom: EEPROM,

    /// EEPROM page sized buffer which is used for temporarily storing
    /// read/written.
    page_buffer: Vec<u8>,
}

impl BlockStorage {
    pub fn new(eeprom: EEPROM) -> Self {
        let mut page_buffer = Vec::new();
        page_buffer.reserve_exact(eeprom.page_size());

        Self {
            state: Mutex::new(BlockStorageState {
                eeprom,
                page_buffer,
            }),
        }
    }

    /*
    TODO: The underlying EEPROM should error out if writing beyond the end.
    */

    /// TODO: Disallow opening the same file twice as the BlockHandles store
    /// state.
    pub async fn open<'a>(&'a self, id: u32, max_length: usize) -> Result<BlockHandle<'a>> {
        const NUM_BUFFERS_PER_BLOCK: usize = 2;

        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        state.eeprom.read(0, &mut state.page_buffer).await?;

        let mut dir = Directory::create(&mut state.page_buffer);

        // Number of pages we will allocate for one buffer of the block.
        let num_pages = common::ceil_div(max_length + BLOCK_OVERHEAD, state.eeprom.page_size());

        // let allocated_bytes = 2 * state.eeprom.page_size() * num_pages;

        let mut next_offset = 0;
        for entry in dir.entries() {
            if entry.id == id {
                if entry.num_pages as usize >= num_pages {
                    return Ok(BlockHandle {
                        storage: self,
                        offset: next_offset,
                        buffer_length: (entry.num_pages as usize) * state.eeprom.page_size(),
                        num_buffers: NUM_BUFFERS_PER_BLOCK,
                        last_counter: None,
                    });
                } else {
                    return Err(BlockOpenError::ExistingFileTooSmall.into());
                }
            }

            next_offset +=
                NUM_BUFFERS_PER_BLOCK * (entry.num_pages as usize) * state.eeprom.page_size();
        }

        // If we are here, then we need to create a new file.

        let buffer_length = num_pages * state.eeprom.page_size();

        let allocated_length = NUM_BUFFERS_PER_BLOCK * buffer_length;
        if next_offset + allocated_length > state.eeprom.total_size() {
            return Err(BlockOpenError::OutOfSpace.into());
        }

        let entry = DirectoryEntry {
            id,
            num_pages: num_pages as u8,
        };
        dir.add_entry(entry);
        dir.finish();
        state.eeprom.write(0, &state.page_buffer).await?;

        Ok(BlockHandle {
            storage: self,
            offset: next_offset,
            buffer_length,
            num_buffers: NUM_BUFFERS_PER_BLOCK,
            last_counter: None,
        })
    }
}

struct Directory<'a> {
    data: &'a mut [u8],
}

impl<'a> Directory<'a> {
    const VERSION_OFFSET: usize = 0;
    const NUM_FILES_OFFSET: usize = 1;
    const ENTRIES_START_OFFSET: usize = 2;

    const CRC_SIZE: usize = 2;

    pub fn create(data: &'a mut [u8]) -> Self {
        let mut inst = Self { data };
        if !inst.is_valid() {
            for i in 0..inst.data.len() {
                inst.data[0] = 0;
            }

            inst.data[Self::VERSION_OFFSET] = 1;
            inst.data[Self::NUM_FILES_OFFSET] = 0;
        }

        inst
    }

    #[inline(always)]
    fn version(&self) -> u8 {
        self.data[Self::VERSION_OFFSET]
    }

    fn num_blocks(&self) -> usize {
        self.data[Self::NUM_FILES_OFFSET] as usize
    }

    pub fn entries(&self) -> &[DirectoryEntry] {
        // TODO: Make this safe.
        unsafe {
            core::slice::from_raw_parts(
                core::mem::transmute(&self.data[Self::ENTRIES_START_OFFSET]),
                self.num_blocks(),
            )
        }
    }

    pub fn add_entry(&mut self, entry: DirectoryEntry) {
        let entries_end_offset =
            Self::ENTRIES_START_OFFSET + core::mem::size_of::<DirectoryEntry>() * self.num_blocks();

        *array_mut_ref![self.data, entries_end_offset, 4] = entry.id.to_le_bytes();
        self.data[entries_end_offset + 4] = entry.num_pages;

        self.data[Self::NUM_FILES_OFFSET] += 1;
    }

    pub fn finish(&mut self) {
        self.set_stored_crc(self.calculate_crc());
    }

    fn stored_crc_offset(&self) -> usize {
        self.data.len() - Directory::CRC_SIZE
    }

    fn stored_crc(&self) -> u16 {
        u16::from_le_bytes(*array_ref![
            self.data,
            self.stored_crc_offset(),
            Directory::CRC_SIZE
        ])
    }

    fn set_stored_crc(&mut self, value: u16) {
        *array_mut_ref![self.data, self.stored_crc_offset(), Directory::CRC_SIZE] =
            value.to_le_bytes();
    }

    fn calculate_crc(&self) -> u16 {
        crypto::checksum::crc16::crc16_lut(&self.data[0..self.stored_crc_offset()])
    }

    fn is_valid(&self) -> bool {
        if self.version() != 1 {
            return false;
        }

        let entries_end_offset =
            Self::ENTRIES_START_OFFSET + core::mem::size_of::<DirectoryEntry>() * self.num_blocks();
        if entries_end_offset > self.stored_crc_offset() {
            return false;
        }

        if self.calculate_crc() != self.stored_crc() {
            return false;
        }

        true
    }
}

#[repr(packed)]
struct DirectoryEntry {
    id: u32,
    num_pages: u8,
}

#[derive(Default, Clone, Copy)]
struct BlockHeader {
    write_count: u32,
    length: u16,
}

pub struct BlockHandle<'a> {
    storage: &'a BlockStorage,

    offset: usize,

    /// Number of bytes allocated for one copy of the block.
    /// The end offset of each block will be offset + num_buffers*buffer_length
    buffer_length: usize,

    num_buffers: usize,

    /// If known, the number of times this block has been written.
    last_counter: Option<u32>,
}

#[derive(Clone, Copy, Debug, Errable)]
#[repr(u32)]
pub enum BlockReadError {
    NoValidData,
    Overflow,
}

#[derive(Clone, Copy, Debug, Errable)]
#[repr(u32)]
pub enum BlockWriteError {
    Overflow,
    WriteBeforeRead,
}

impl<'a> BlockHandle<'a> {
    /// NOTE: 'data' MUST be large enough to store the max_length of the
    pub async fn read(&mut self, data: &mut [u8]) -> Result<usize> {
        let mut state = self.storage.state.lock().await;

        let mut blocks = FixedVec::<(BlockHeader, usize), _>::new([(BlockHeader::default(), 0); 2]);

        let mut next_offset = self.offset;
        for _ in 0..self.num_buffers {
            let header = {
                let mut data = [0u8; 6];
                state.eeprom.read(next_offset, &mut data).await?;
                BlockHeader {
                    write_count: u32::from_le_bytes(*array_ref![data, 0, 4]),
                    length: u16::from_le_bytes(*array_ref![data, 4, 2]),
                }
            };

            blocks.push((header, next_offset));

            next_offset += self.buffer_length;
        }

        // Sort in descending order.
        // TODO: Use a more code size efficient sort like bubble sort.
        blocks.sort_by(|a, b| b.0.write_count.cmp(&a.0.write_count));

        let max_data_length = self.buffer_length - BLOCK_OVERHEAD;

        for (header, offset) in blocks.deref() {
            let data_length = header.length as usize;
            if data_length > max_data_length {
                continue;
            }

            // TODO: Respect any max transfer length supported to the I2CController (may
            // need to split up the transfer). ^ for the purposes of cache
            // friendiness with the hasher, this might be helpful anyway?

            // TODO: Only return this if the data actually ends up being valid (CRC-wise).
            if data.len() < data_length {
                return Err(BlockReadError::Overflow.into());
            }

            state
                .eeprom
                .read(*offset + 6, &mut data[0..data_length])
                .await?;

            let expected_hash = crypto::checksum::crc16::crc16_lut(&data[0..data_length]);

            let hash = {
                let mut buf = [0u8; 2];
                state
                    .eeprom
                    .read(*offset + self.buffer_length - 2, &mut buf)
                    .await?;
                u16::from_le_bytes(buf)
            };

            if hash != expected_hash {
                continue;
            }

            self.last_counter = Some(header.write_count);
            return Ok(data_length);
        }

        self.last_counter = Some(0);

        Err(BlockReadError::NoValidData.into())
    }

    /// NOTE: Before reading, a user must have already read
    pub async fn write(&mut self, mut data: &[u8]) -> Result<()> {
        let last_counter = self
            .last_counter
            .ok_or_else(|| Error::from(BlockWriteError::WriteBeforeRead))?;

        let mut offset =
            self.offset + self.buffer_length * (last_counter as usize % self.num_buffers);
        let counter = last_counter + 1;

        let mut first = true;
        let mut crc_state = 0;
        let mut crc_written = false;

        let mut state_guard = self.storage.state.lock().await;
        let state = &mut *state_guard;

        // NOTE: If the write requires fewer pages than are allocated for the buffer,
        // then the excess pages won't be written.
        while !crc_written {
            let mut page_i = 0;

            if first {
                *array_mut_ref![state.page_buffer, 0, 4] = counter.to_le_bytes();
                *array_mut_ref![state.page_buffer, 4, 2] = (data.len() as u16).to_le_bytes(); // TODO: Error out if too big.
                page_i = 6;
                first = false;
            }

            let n = core::cmp::min(state.page_buffer.len() - page_i, data.len());
            state.page_buffer[page_i..(page_i + n)].copy_from_slice(&data[0..n]);
            page_i += n;
            data = &data[n..];

            crc_state = crc16_incremental_lut(crc_state, &state.page_buffer[0..page_i]);

            if state.page_buffer.len() - page_i >= 2 {
                *array_mut_ref![state.page_buffer, page_i, 2] = crc_state.to_le_bytes();
                page_i += 2;
                crc_written = true;
            }

            // Pad with zeros.
            while page_i < state.page_buffer.len() {
                state.page_buffer[page_i] = 0;
            }

            // Write page
            state.eeprom.write(offset, &state.page_buffer).await?;

            offset += state.eeprom.page_size();
        }

        self.last_counter = Some(counter);

        Ok(())
    }
}
