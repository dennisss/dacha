use common::{errors::*, fixed::vec::FixedVec, list::Appendable};
use crypto::checksum::crc16::crc16_incremental_lut;

// CRC-16 AUG-CCITT
// We choose an algorithm with an init value so that empty data with all zeros
// or ones doesn't appear to be valid data.
const CRC_INITIAL_STATE: u16 = 0x1d0f;

/// Max length of a single parameter's value (4095)
const PARAM_LENGTH_LIMIT: usize = 0x0FFF;

#[derive(Clone, Copy, Debug, Errable, PartialEq)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum ParamStorageError {
    /// When returned by ParamStorage::create(), this means that the memory
    /// contains an entry for a param if which doesn't exist in the registry.
    ///
    /// When returnged by ParamStorage::read(), this means that the requested
    /// param id is not registered in storage (this is different than the param
    /// having no value yet).
    UnknownParamId,

    /// There is not currently enough space to write the value passed to
    /// ParamStorage::write(). This could be due to too much fragmentation of
    /// values.
    OutOfSpace,

    /// ParamStorage::write() was called with a value which is too large to
    /// represent in serialized form.
    ValueTooLarge,
}

impl core::fmt::Display for ParamStorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Interface for reading/writing from/to a segment of non-volatile memory.
pub trait ParamMemoryController {
    /// Gets the total number of bytes that can be stored in this memory.
    fn len(&self) -> usize;

    /// Smallest length of memory which can be independently erased.
    /// We assume that the the first page is located at offsets i*PAGE_SIZE over
    /// the entire length of the memory.
    fn page_size(&self) -> usize;

    /// All writes to this memory should be at an offset aligned to this number
    /// of bytes and the write length should be a multiple of this size.
    ///
    /// page_size MUST be divisible by write_alignment.
    fn write_alignment(&self) -> usize;

    /// Checks whether or not we are allowed to write to a given offset.
    ///
    /// We should always be able to write to the start of a page as that implies
    /// also erasing the page. But, if a page was already partially or
    /// incompletely written in a previous write attempt, it may not be possible
    /// to continue overwriting a part of the page.
    fn can_write_to_offset(&self, offset: usize) -> bool;

    fn get<'a>(&'a self) -> &'a [u8];

    fn read(&self, offset: usize, out: &mut [u8]);

    /// Writes 'data' at the 'offset' position in memory.
    ///
    /// A write to the start of a page should erase it entirely before any new
    /// data is written. Subsequent writes inside of the page should append
    /// data to the page without erasing previously written data.
    fn write(&mut self, offset: usize, data: &[u8]);
}

// TODO: Have well defined behavior for what to do if we see an unknown id in
// the params.
pub trait ParamRegistry {
    fn param_handle(&self, id: u32) -> Option<&ParamHandle>;

    fn param_handle_mut(&mut self, id: u32) -> Option<&mut ParamHandle>;

    fn param_at_index(&self, index: usize) -> &ParamHandle;

    fn param_at_index_mut(&mut self, index: usize) -> &mut ParamHandle;

    fn num_params(&self) -> usize;
}

impl<T: AsRef<[ParamHandle]> + AsMut<[ParamHandle]>> ParamRegistry for T {
    fn param_handle(&self, id: u32) -> Option<&ParamHandle> {
        for param in self.as_ref().iter() {
            if param.id == id {
                return Some(param);
            }
        }

        None
    }

    fn param_handle_mut(&mut self, id: u32) -> Option<&mut ParamHandle> {
        for param in self.as_mut().iter_mut() {
            if param.id == id {
                return Some(param);
            }
        }

        None
    }

    fn param_at_index(&self, index: usize) -> &ParamHandle {
        &self.as_ref()[index]
    }

    fn param_at_index_mut(&mut self, index: usize) -> &mut ParamHandle {
        &mut self.as_mut()[index]
    }

    fn num_params(&self) -> usize {
        self.as_ref().len()
    }
}

// TODO: We must ensure that a single handle is only ever registered with a
// single ParamStorage instance (and that a param handle is only use with the
// ParamStorage instance it is registered in).
pub struct ParamHandle {
    id: u32,
    latest_entry: Option<ParamEntryHandle>,

    /// NOTE: This is only used during the initialization phase of the
    /// ParamStorage object so could be moved to stack memory.
    pending_entry: Option<ParamEntryHandle>,
}

impl ParamHandle {
    pub const fn new(id: u32) -> Self {
        Self {
            id,
            latest_entry: None,
            pending_entry: None,
        }
    }
}

/// Reference to the value of a param stored inside an entry in raw storage.
#[derive(Clone)]
struct ParamEntryHandle {
    /// Absolute index of the page storing this entry.
    /// NOTE: May be > the # of pages in memory.
    page_index: usize,

    checksum: u16,

    /// Start position of the value field of this entry relative to the start of
    /// the storage memory.
    value_absolute_offset: usize,

    value_length: usize,
}

pub struct ParamStorage<Memory, Registry> {
    memory: Memory,

    registry: Registry,

    current_position: ParamWriteCursor,
}

#[derive(Clone, Debug)]
struct ParamWriteCursor {
    /// Absolute index of the page which we are currently writing.
    /// NOTE: May be > the # of pages in memory.
    page_index: usize,

    /// Offset relative to the start of the current page at which we can next
    /// write new entries.
    page_offset: usize,

    last_entry_id: Option<u32>,
}

impl<Memory: ParamMemoryController, Registry: ParamRegistry> ParamStorage<Memory, Registry> {
    /// Instantiates a new ParamStorage instance.
    ///
    /// On instantiation this scans the contents of the given memory to find all
    /// existing params values.
    pub fn create(memory: Memory, mut registry: Registry) -> Result<Self> {
        let num_pages = memory.len() / memory.page_size();

        // To gurantee atomic writes, we must be able to have at least one backup page
        // while erasing the other.
        assert!(num_pages >= 2);

        // TODO: Clear the initial value of all the the latest_entry and pending_entry
        // in the registry.

        let data = memory.get();

        let mut current_position = ParamWriteCursor {
            page_index: 0,
            page_offset: 0,
            last_entry_id: None,
        };

        for page_i in 0..num_pages {
            let page_start = page_i * memory.page_size();
            let page_data = &data[page_start..(page_start + memory.page_size())];

            let page_index = u32::from_le_bytes(*array_ref![page_data, 0, 4]) as usize;
            if page_index % num_pages != page_i {
                continue;
            }
            let mut page_offset = 4;
            page_offset +=
                common::block_size_remainder(memory.write_alignment() as u64, page_offset as u64)
                    as usize;

            let mut page_last_entry_id = None;

            let mut checkpoint_seen = false;

            while page_offset < page_data.len() {
                match ParamEntry::parse(
                    &page_data[page_offset..],
                    page_index as u32,
                    page_last_entry_id,
                ) {
                    Some(entry) => {
                        if entry.checkpoint && !checkpoint_seen {
                            checkpoint_seen = true;

                            // Make all pending_entry fields for this page the latest_entry of each
                            // param.
                            for i in 0..registry.num_params() {
                                let param = registry.param_at_index_mut(i);
                                if let Some(pending_entry) = param.pending_entry.take() {
                                    if pending_entry.page_index == page_index {
                                        param.latest_entry = Some(pending_entry);
                                    }
                                }
                            }
                        }

                        page_last_entry_id = Some(entry.id);

                        let param = match registry.param_handle_mut(entry.id) {
                            Some(v) => v,
                            None => return Err(ParamStorageError::UnknownParamId.into()),
                        };

                        let is_newer = {
                            if let Some(latest_entry) = &param.latest_entry {
                                page_index >= latest_entry.page_index
                            } else {
                                true
                            }
                        };

                        if is_newer {
                            let handle = Some(ParamEntryHandle {
                                page_index,
                                checksum: entry.checksum,
                                value_absolute_offset: page_start + page_offset + entry.value_start,
                                value_length: entry.value_length,
                            });

                            if checkpoint_seen {
                                param.latest_entry = handle;
                            } else {
                                param.pending_entry = handle;
                            }
                        }

                        page_offset += entry.total_size;
                        page_offset += common::block_size_remainder(
                            memory.write_alignment() as u64,
                            page_offset as u64,
                        ) as usize;
                    }
                    None => {
                        // No more valid data on this page.
                        break;
                    }
                }
            }

            let is_valid_page = checkpoint_seen && page_last_entry_id.is_some();

            if is_valid_page && !memory.can_write_to_offset(page_start + page_offset) {
                current_position.page_offset = memory.page_size();
            }

            if is_valid_page && page_index >= current_position.page_index {
                current_position.page_index = page_index;
                current_position.page_offset = page_offset;
                current_position.last_entry_id = page_last_entry_id;
            }
        }

        Ok(Self {
            memory,
            registry,
            current_position,
        })
    }

    pub fn get(&self, param_id: u32) -> Result<Option<&[u8]>> {
        let param = match self.registry.param_handle(param_id) {
            Some(v) => v,
            None => return Err(ParamStorageError::UnknownParamId.into()),
        };

        if let Some(entry) = &param.latest_entry {
            let data = self.memory.get();

            let start = entry.value_absolute_offset;
            let end = start + entry.value_length;

            Ok(Some(&data[start..end]))
        } else {
            Ok(None)
        }
    }

    pub fn write(&mut self, param_id: u32, value: &[u8]) -> Result<()> {
        let page_size = self.memory.page_size();
        let num_pages = self.memory.len() / self.memory.page_size();

        // TODO: Think about whether or not this is enough iteraitons.
        let mut num_erases = 0;
        while num_erases < 2 {
            if self.current_position.page_offset == 0 {
                let mut current_page_start_offset =
                    (self.current_position.page_index % num_pages) * self.memory.page_size();

                // Write the page counter to the beginning of the page.
                // This should also trigger the entire page to be erased within this write()
                // call.
                let page_index_data = (self.current_position.page_index as u32).to_le_bytes();
                self.memory
                    .write(current_page_start_offset, &page_index_data);
                self.current_position.page_offset += 4;

                self.current_position.page_offset += common::block_size_remainder(
                    self.memory.write_alignment() as u64,
                    self.current_position.page_offset as u64,
                ) as usize;

                num_erases += 1;

                let mut last_param_to_rewrite = 0;
                for i in 0..self.registry.num_params() {
                    let param = self.registry.param_at_index(i);
                    if let Some(entry) = &param.latest_entry {
                        if (entry.page_index % num_pages)
                            == (self.current_position.page_index + 1) % num_pages
                        {
                            last_param_to_rewrite = i;
                        }
                    }
                }

                // Every param who's latest value is stored on the page immediately after this
                // one must be moved to the new page.
                for i in 0..self.registry.num_params() {
                    let param = self.registry.param_at_index(i);
                    if let Some(entry) = &param.latest_entry {
                        // We can't re-write a page if there are still live values on it. This
                        // should never happen. (NOTE: If this fails then
                        // it's already too late as we already deleted the
                        // page).
                        assert!(entry.page_index != self.current_position.page_index);

                        // TODO: If the moved param is small enough to it in the remaining space on
                        // the previous page, attempt to move it there first.

                        // TODO: If the param id is the same id as the one we are about to re-write,
                        // we don't need to copy it (if the new value can
                        // fit on the page).

                        // TODO: Write checkpoint if it is the last entry.
                        if (entry.page_index % num_pages)
                            == (self.current_position.page_index + 1) % num_pages
                        {
                            // NOTE: This should always fit in the new page because it fit in the
                            // old page.
                            let new_entry = Self::write_entry(
                                param.id,
                                ParamEntryValue::Existing(entry.clone()),
                                i == last_param_to_rewrite,
                                &mut self.memory,
                                &mut self.current_position,
                            )
                            .unwrap();

                            self.registry
                                .param_handle_mut(param.id)
                                .unwrap()
                                .latest_entry = Some(new_entry);
                        }
                    }
                }
            }

            let param = match self.registry.param_handle_mut(param_id) {
                Some(v) => v,
                None => return Err(ParamStorageError::UnknownParamId.into()),
            };

            if let Some(new_entry) = Self::write_entry(
                param.id,
                ParamEntryValue::Buffer(value),
                true,
                &mut self.memory,
                &mut self.current_position,
            ) {
                param.latest_entry = Some(new_entry);
                return Ok(());
            } else {
                // If it doesn't fit in the current page, try deleting the next page and fitting
                // it into there.
                self.current_position.page_index += 1;
                self.current_position.page_offset = 0;
                self.current_position.last_entry_id = None;
                continue;
            }
        }

        Err(ParamStorageError::OutOfSpace.into())
    }

    /// Writes a param entry at the given current position in non-volatile
    /// memory.
    ///
    /// Returns a handle to the newly written entry and updates the current
    /// position to be immediately after the entry. If the entry can't fit in
    /// the current page, None is returned and the position is not updates.
    fn write_entry(
        id: u32,
        value: ParamEntryValue,
        checkpoint: bool,
        memory: &mut Memory,
        current_position: &mut ParamWriteCursor,
    ) -> Option<ParamEntryHandle> {
        const BUFFER_SIZE: usize = 64;
        assert!(BUFFER_SIZE % memory.write_alignment() == 0);

        let mut buffer = FixedVec::<u8, BUFFER_SIZE>::new();

        let checksum = match &value {
            ParamEntryValue::Buffer(value) => {
                let mut sum = CRC_INITIAL_STATE;
                sum = crc16_incremental_lut(sum, &id.to_le_bytes());
                sum = crc16_incremental_lut(sum, value);
                sum
            }
            ParamEntryValue::Existing(entry) => entry.checksum,
        };

        buffer.extend_from_slice(
            &ParamEntryHeader {
                stored_id: if Some(id) != current_position.last_entry_id {
                    Some(id)
                } else {
                    None
                },
                checksum,
                value_length: value.len(),
                checkpoint,
            }
            .serialize(),
        );

        // Before writing, verify that the write will fit within one page.
        if current_position.page_offset + buffer.len() + value.len() > memory.page_size() {
            return None;
        }

        let num_pages = memory.len() / memory.page_size();
        let page_size = memory.page_size();

        // Current position in the value we are reading (relative to the beginning of
        // the value)
        let mut value_offset = 0;

        // Current position at which we are writing bytes.
        let mut write_offset =
            (current_position.page_index % num_pages) * page_size + current_position.page_offset;

        // Offset of the value in the output memory (immediately after the header).
        let value_absolute_offset = write_offset + buffer.len();

        while value_offset < value.len() {
            let buffer_start = buffer.len();

            let n = core::cmp::min(BUFFER_SIZE - buffer_start, value.len() - value_offset);
            buffer.resize(buffer_start + n, 0);

            match value {
                ParamEntryValue::Existing(ref entry) => {
                    memory.read(
                        entry.value_absolute_offset + value_offset,
                        &mut buffer[buffer_start..],
                    );
                }
                ParamEntryValue::Buffer(value) => {
                    buffer[buffer_start..]
                        .copy_from_slice(&value[value_offset..(value_offset + n)]);
                }
            }
            value_offset += n;

            let padding_amount =
                common::block_size_remainder(memory.write_alignment() as u64, buffer.len() as u64)
                    as usize;
            buffer.resize(buffer.len() + padding_amount, 0);

            memory.write(write_offset, &buffer);
            write_offset += buffer.len();
            current_position.page_offset += buffer.len();

            buffer.clear();
        }

        current_position.last_entry_id = Some(id);

        Some(ParamEntryHandle {
            page_index: current_position.page_index,
            checksum,
            value_absolute_offset,
            value_length: value.len(),
        })
    }
}

enum ParamEntryValue<'a> {
    /// The value should be taken from an existing param.
    Existing(ParamEntryHandle),

    /// The value is stored in a buffer in RAM.
    Buffer(&'a [u8]),
}

impl<'a> ParamEntryValue<'a> {
    fn len(&self) -> usize {
        match self {
            ParamEntryValue::Buffer(v) => v.len(),
            ParamEntryValue::Existing(handle) => handle.value_length,
        }
    }
}

// 4 bits
define_bit_flags!(ParamEntryFlags u32 {
    // Parity bit which may be set in order to make the number of ones in the flags be odd.
    PARITY = 1 << 3,
    STORE_ID = 1 << 2,
    CHECKPOINT = 1 << 1,
    RESERVED = 1 << 0
});

struct ParamEntryHeader {
    stored_id: Option<u32>,
    checkpoint: bool,
    checksum: u16,
    value_length: usize,
}

impl ParamEntryHeader {
    fn entry_size(&self) -> usize {
        let mut total = 4 + self.value_length;
        if self.stored_id.is_some() {
            total += 4;
        }

        total
    }

    fn serialize(&self) -> FixedVec<u8, 8> {
        let mut out = FixedVec::new();

        let mut flags = ParamEntryFlags::RESERVED;

        if self.stored_id.is_some() {
            flags = flags | ParamEntryFlags::STORE_ID;
        }

        if self.checkpoint {
            flags = flags | ParamEntryFlags::CHECKPOINT;
        }

        if flags.to_raw().count_ones() % 2 == 0 {
            flags = flags | ParamEntryFlags::PARITY;
        }

        let header =
            (flags.to_raw() << 28) | ((self.value_length as u32) << 16) | (self.checksum as u32);

        out.extend_from_slice(&header.to_le_bytes());

        if let Some(id) = self.stored_id {
            out.extend_from_slice(&id.to_le_bytes());
        }

        out
    }
}

/// Representation of a ParamEntry which was decoded from a stream of bytes.
struct ParamEntry {
    id: u32,

    checksum: u16,

    /// Offset relative to page_offset at which the value of this data begins.
    value_start: usize,

    /// Total length of the value which begins at value_start.
    value_length: usize,

    /// Total number of bytes used by this entry.
    total_size: usize,

    /// Whether or not the 'checkpoint' flag was set for this entry.
    checkpoint: bool,
}

impl ParamEntry {
    /// Returns None if we couldn't parse a valid entry.
    fn parse(data: &[u8], page_index: u32, last_entry_id: Option<u32>) -> Option<ParamEntry> {
        let mut offset = 0;

        let entry_header = {
            if offset + 4 > data.len() {
                return None;
            }

            u32::from_le_bytes(*array_ref![data, offset, 4])
        };
        offset += 4;

        // Top 4 bits reserved for flags.
        let flags = ParamEntryFlags::from_raw(entry_header >> 28);
        if flags.to_raw().count_ones() % 2 != 1 {
            return None;
        }

        let value_length = ((entry_header >> 16) & ((1 << 12) - 1)) as usize;
        let expected_checksum = (entry_header & 0xFFFF) as u16;

        let id = {
            if flags.contains(ParamEntryFlags::STORE_ID) {
                if offset + 4 > data.len() {
                    return None;
                }

                let id = u32::from_le_bytes(*array_ref![data, offset, 4]);
                offset += 4;
                id
            } else if let Some(id) = last_entry_id {
                id
            } else {
                return None;
            }
        };

        let value_start = offset;

        if offset + value_length > data.len() {
            return None;
        }
        let value = &data[offset..(offset + value_length)];
        offset += value_length;

        let checksum = {
            let mut sum = CRC_INITIAL_STATE;
            sum = crc16_incremental_lut(sum, &id.to_le_bytes());
            sum = crc16_incremental_lut(sum, value);
            sum
        };

        if expected_checksum != checksum {
            return None;
        }

        Some(Self {
            id,
            checksum,
            value_start,
            value_length,
            total_size: offset,
            checkpoint: flags.contains(ParamEntryFlags::CHECKPOINT),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use alloc::vec::Vec;

    struct SimpleMemory {
        data: Vec<u8>,
    }

    const PAGE_SIZE: usize = 64;
    const BLOCK_SIZE: usize = 4;

    impl SimpleMemory {
        fn new() -> Self {
            Self {
                data: vec![0u8; PAGE_SIZE * 2],
            }
        }
    }

    impl ParamMemoryController for &mut SimpleMemory {
        fn page_size(&self) -> usize {
            PAGE_SIZE
        }

        fn write_alignment(&self) -> usize {
            BLOCK_SIZE
        }

        fn len(&self) -> usize {
            self.data.len()
        }

        fn get(&self) -> &[u8] {
            &self.data
        }

        fn can_write_to_offset(&self, offset: usize) -> bool {
            true
        }

        fn read(&self, offset: usize, out: &mut [u8]) {
            out.copy_from_slice(&self.data[offset..(offset + out.len())]);
        }

        fn write(&mut self, offset: usize, data: &[u8]) {
            self.data[offset..(offset + data.len())].copy_from_slice(data);
        }
    }

    #[test]
    fn works() {
        let mut memory = SimpleMemory::new();

        {
            let mut params = vec![ParamHandle::new(1), ParamHandle::new(2)];
            let mut storage = ParamStorage::create(&mut memory, params).unwrap();

            storage.write(1, &[10, 20, 30]).unwrap();
            assert_eq!(storage.get(1).unwrap(), Some(&[10, 20, 30][..]));
            assert_eq!(storage.get(2).unwrap(), None);

            // Verify can maintain different values for different ids
            storage.write(2, &[11, 21, 31]).unwrap();
            assert_eq!(storage.get(1).unwrap(), Some(&[10, 20, 30][..]));
            assert_eq!(storage.get(2).unwrap(), Some(&[11, 21, 31][..]));

            // Verify that writing 2 again retains the value of 1.
            storage.write(2, &[101, 102, 103, 104, 105]).unwrap();
            assert_eq!(storage.get(1).unwrap(), Some(&[10, 20, 30][..]));
            assert_eq!(
                storage.get(2).unwrap(),
                Some(&[101, 102, 103, 104, 105][..])
            );

            println!("{:?}", storage.current_position);
        }

        println!("{:?}", memory.data);

        {
            // Verify that re-opening memory with multiple entries picks the latest entry.
            let mut params = vec![ParamHandle::new(1), ParamHandle::new(2)];
            let mut storage = ParamStorage::create(&mut memory, params).unwrap();
            assert_eq!(storage.get(1).unwrap(), Some(&[10, 20, 30][..]));
            assert_eq!(
                storage.get(2).unwrap(),
                Some(&[101, 102, 103, 104, 105][..])
            );

            // TODO: Verify this position is the same as from before the open operation.
            println!("{:?}", storage.current_position);

            storage
                .write(2, &[70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80])
                .unwrap();

            assert_eq!(storage.get(1).unwrap(), Some(&[10, 20, 30][..]));
            assert_eq!(
                storage.get(2).unwrap(),
                Some(&[70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80][..])
            );

            // This should trigger overflow to the
            storage.write(1, &[81, 82, 83, 84, 85, 86, 87, 88]);

            // Should be at 39 at the second page (although I guess if we intent on
            // immediately writing a change to one of them, then we don't need to write it
            // again).

            assert_eq!(
                storage.get(1).unwrap(),
                Some(&[81, 82, 83, 84, 85, 86, 87, 88][..])
            );
            assert_eq!(
                storage.get(2).unwrap(),
                Some(&[70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80][..])
            );

            println!("{:?}", storage.current_position);

            for i in 0..1000 {
                storage.write(1, b"xxxxxxxxxx");
            }

            assert_eq!(storage.get(1).unwrap(), Some(&b"xxxxxxxxxx"[..]));
            assert_eq!(
                storage.get(2).unwrap(),
                Some(&[70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80][..])
            );
        }

        // TODO: Test that we don't continue writing to a partially written page
        // if not allowed by the memory controller.

        // TODO: Test overflowing back to the first page and verifying that
        // values on the first page are used instead of the second page (uses
        // index rather than page postiion order).

        // TODO: Test writes that are very large and require multiple buffer
        // passes.

        // TODO: Test having a single empty entry in a block of size 0 (so just
        // barely fits).
    }
}
