use core::cmp::Ordering;
use core::ops::Deref;

use common::errors::*;
use common::fixed::vec::FixedVec;
use crypto::checksum::crc16::crc16_incremental_lut;
use crypto::hasher::Hasher;
use executor::mutex::Mutex;

use crate::eeprom::EEPROM;

// TODO: Support making this dynamic.
const PAGE_SIZE: usize = 64;

// 4 bytes for write_count
// 2 bytes for length
// 2 bytes for CRC-16
const BLOCK_OVERHEAD: usize = BLOCK_HEADER_SIZE + 2;

const BLOCK_HEADER_SIZE: usize = 2 + 4;

// CRC-16 AUG-CCITT
// We choose an algorithm with an init value so that empty data with all zeros
// or ones doesn't appear to be valid data.
const CRC_INITIAL_STATE: u16 = 0x1d0f;

#[derive(Clone, Copy, Debug, Errable, PartialEq)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum BlockStorageError {
    OutOfSpace,
    ExistingFileTooSmall,
    NoValidData,
    Overflow,

    /// The user attempted to read
    WriteBeforeRead,
}

impl core::fmt::Display for BlockStorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct BlockStorage<E> {
    state: Mutex<BlockStorageState<E>>,
}

struct BlockStorageState<E> {
    eeprom: E,

    /// EEPROM page sized buffer which is used for temporarily storing
    /// read/written.
    page_buffer: [u8; PAGE_SIZE],
}

impl<E: EEPROM> BlockStorage<E> {
    pub fn new(eeprom: E) -> Self {
        let mut page_buffer = [0u8; PAGE_SIZE];

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
    pub async fn open<'a>(&'a self, id: u32, max_length: usize) -> Result<BlockHandle<'a, E>> {
        const NUM_BUFFERS_PER_BLOCK: usize = 2;

        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        state.eeprom.read(0, &mut state.page_buffer).await?;

        let mut dir = Directory::create(&mut state.page_buffer);

        // Number of pages we will allocate for one buffer of the block.
        let num_pages = common::ceil_div(max_length + BLOCK_OVERHEAD, state.eeprom.page_size());

        // The first file starts after the directory page.
        let mut next_offset = state.eeprom.page_size();

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
                    return Err(BlockStorageError::ExistingFileTooSmall.into());
                }
            }

            next_offset +=
                NUM_BUFFERS_PER_BLOCK * (entry.num_pages as usize) * state.eeprom.page_size();
        }

        // If we are here, then we need to create a new file.

        let buffer_length = num_pages * state.eeprom.page_size();

        let allocated_length = NUM_BUFFERS_PER_BLOCK * buffer_length;
        if next_offset + allocated_length > state.eeprom.total_size() {
            return Err(BlockStorageError::OutOfSpace.into());
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
        crc16_incremental_lut(CRC_INITIAL_STATE, &self.data[0..self.stored_crc_offset()])
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

pub struct BlockHandle<'a, E> {
    storage: &'a BlockStorage<E>,

    offset: usize,

    /// Number of bytes allocated for one copy of the block.
    /// The end offset of each block will be offset + num_buffers*buffer_length
    buffer_length: usize,

    num_buffers: usize,

    /// If known, the number of times this block has been written.
    last_counter: Option<u32>,
}

impl<'a, E: EEPROM> BlockHandle<'a, E> {
    /// Reads the current value of the block into the given buffer.
    ///
    /// NOTE: 'data' MUST be large enough to store the max_length of the
    pub async fn read(&mut self, data: &mut [u8]) -> Result<usize> {
        let mut state = self.storage.state.lock().await;

        let mut blocks = FixedVec::<(BlockHeader, u16, usize), 2>::new();

        let mut next_offset = self.offset;
        for _ in 0..self.num_buffers {
            let mut header_data = [0u8; BLOCK_HEADER_SIZE];
            state.eeprom.read(next_offset, &mut header_data).await?;

            let header = {
                BlockHeader {
                    write_count: u32::from_le_bytes(*array_ref![header_data, 0, 4]),
                    length: u16::from_le_bytes(*array_ref![header_data, 4, 2]),
                }
            };

            if header.write_count != 0 {
                let header_crc = crc16_incremental_lut(CRC_INITIAL_STATE, &header_data);

                blocks.push((header, header_crc, next_offset));
            }

            next_offset += self.buffer_length;
        }

        // Sort in descending order.
        // TODO: Use a more code size efficient sort like bubble sort.
        common::sort::bubble_sort_by(blocks.as_mut(), |a, b| {
            b.0.write_count.cmp(&a.0.write_count)
        });

        let max_data_length = self.buffer_length - BLOCK_OVERHEAD;

        for (header, header_crc, offset) in blocks.deref() {
            let data_length = header.length as usize;
            if data_length > max_data_length {
                continue;
            }

            // TODO: Respect any max transfer length supported to the I2CController (may
            // need to split up the transfer). ^ for the purposes of cache
            // friendiness with the hasher, this might be helpful anyway?

            // TODO: Only return this if the data actually ends up being valid (CRC-wise).
            if data.len() < data_length {
                return Err(BlockStorageError::Overflow.into());
            }

            state
                .eeprom
                .read(*offset + BLOCK_HEADER_SIZE, &mut data[0..data_length])
                .await?;

            let expected_hash = crc16_incremental_lut(*header_crc, &data[0..data_length]);

            let hash = {
                let mut buf = [0u8; 2];
                state
                    .eeprom
                    .read(*offset + BLOCK_HEADER_SIZE + data_length, &mut buf)
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

        Err(BlockStorageError::NoValidData.into())
    }

    /// NOTE: Before reading, a user must have already read
    pub async fn write(&mut self, mut data: &[u8]) -> Result<()> {
        let last_counter = self
            .last_counter
            .ok_or_else(|| Error::from(BlockStorageError::WriteBeforeRead))?;

        let mut offset =
            self.offset + self.buffer_length * (last_counter as usize % self.num_buffers);
        let counter = last_counter + 1;

        let mut first = true;
        let mut crc_state = CRC_INITIAL_STATE;
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
                page_i += 1;
            }

            // Write page
            state.eeprom.write(offset, &state.page_buffer).await?;

            offset += state.eeprom.page_size();
        }

        self.last_counter = Some(counter);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use alloc::vec;
    use alloc::vec::Vec;
    use core::convert::{AsMut, AsRef};
    use core::future::Future;

    struct FakeEEPROM<D> {
        data: D,
    }

    impl<D> FakeEEPROM<D> {
        pub fn new(data: D) -> Self {
            Self { data }
        }
    }

    impl<D: AsRef<[u8]> + AsMut<[u8]>> EEPROM for FakeEEPROM<D> {
        type ReadFuture<'a>
        where
            D: 'a,
        = impl Future<Output = Result<()>> + 'a;
        type WriteFuture<'a>
        where
            D: 'a,
        = impl Future<Output = Result<()>> + 'a;

        fn total_size(&self) -> usize {
            self.data.as_ref().len()
        }

        fn page_size(&self) -> usize {
            64
        }

        fn read<'a>(&'a mut self, offset: usize, data: &'a mut [u8]) -> Self::ReadFuture<'a> {
            async move {
                data.copy_from_slice(&self.data.as_mut()[offset..(offset + data.len())]);
                Ok(())
            }
        }

        fn write<'a>(&'a mut self, offset: usize, data: &'a [u8]) -> Self::WriteFuture<'a> {
            async move {
                self.data.as_mut()[offset..(offset + data.len())].copy_from_slice(data);
                Ok(())
            }
        }
    }

    #[test]
    fn single_block() -> Result<()> {
        executor::run(single_block_inner())?
    }

    async fn single_block_inner() -> Result<()> {
        let mut eeprom_data = vec![0u8; 4096];

        // Start with empty eeprom. Verify file doesn't exist yet and create first write
        // for it.
        {
            let eeprom = FakeEEPROM::new(&mut eeprom_data);
            let store = BlockStorage::new(eeprom);

            let mut handle1 = store.open(1, 96).await?;

            let mut buf = vec![0u8; 100];

            assert_eq!(
                handle1
                    .read(&mut buf)
                    .await
                    .expect_err("Read should fail")
                    .downcast_ref::<BlockStorageError>()
                    .unwrap(),
                &BlockStorageError::NoValidData
            );

            handle1.write(b"Apple").await?;

            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Apple".len());
            assert_eq!(&buf[0..n], b"Apple");
        }

        // Re-open eeprom to verify that data was persisted.
        // Also performing a second
        {
            let eeprom = FakeEEPROM::new(&mut eeprom_data);
            let store = BlockStorage::new(eeprom);

            let mut handle1 = store.open(1, 96).await?;

            let mut buf = vec![0u8; 100];
            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Apple".len());
            assert_eq!(&buf[0..n], b"Apple");

            handle1.write(b"Watermelon").await?;

            let mut buf = vec![0u8; 100];
            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Watermelon".len());
            assert_eq!(&buf[0..n], b"Watermelon");
        }

        // Re-open and verify we can still read the file's value.
        // Then create a second file that is very big.
        {
            let eeprom = FakeEEPROM::new(&mut eeprom_data);
            let store = BlockStorage::new(eeprom);

            let mut handle1 = store.open(1, 96).await?;

            let mut buf = vec![0u8; 100];
            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Watermelon".len());
            assert_eq!(&buf[0..n], b"Watermelon");

            let mut handle2 = store.open(2, 256).await?;
            let mut buf = vec![0u8; 256];
            assert_eq!(
                handle2
                    .read(&mut buf)
                    .await
                    .expect_err("Read should fail")
                    .downcast_ref::<BlockStorageError>()
                    .unwrap(),
                &BlockStorageError::NoValidData
            );

            handle2.write(&[0xBF; 200]).await?;

            let mut buf = vec![0u8; 100];
            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Watermelon".len());
            assert_eq!(&buf[0..n], b"Watermelon");

            let mut buf = vec![0u8; 256];
            let n = handle2.read(&mut buf).await?;
            assert_eq!(n, 200);
            assert_eq!(&buf[0..n], &[0xBF; 200]);
        }

        // Close and verify both files still exist.
        // Open in opposite order to verify it's still ok.
        {
            let eeprom = FakeEEPROM::new(&mut eeprom_data);
            let store = BlockStorage::new(eeprom);

            let mut handle2 = store.open(2, 256).await?;
            let mut handle1 = store.open(1, 96).await?;

            let mut buf = vec![0u8; 256];
            let n = handle2.read(&mut buf).await?;
            assert_eq!(n, 200);
            assert_eq!(&buf[0..n], &[0xBF; 200]);

            let mut buf = vec![0u8; 100];
            let n = handle1.read(&mut buf).await?;
            assert_eq!(n, b"Watermelon".len());
            assert_eq!(&buf[0..n], b"Watermelon");
        }

        Ok(())
    }

    /*
    Things to test.
    - Write to a few files and then close the handles and re-open the files
        - After this we should still have the old data.
    - writing an even and odd number of times.

    - Test that if a write fails, we still can read back the old value.

    */
}
