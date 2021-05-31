use std::collections::VecDeque;

use crate::hpack::header_field::HeaderField;

// TODO: For the purposes of encoding, this should be indexed by name.
pub struct DynamicTable {
    // /// Absolute maximum size of the table as indicated by the protocol using HPACK.
    // ceiling_size: usize,
    
    /// Maximum size of the table. When the size of the table exceeds this value,
    /// entries will be evicted. 
    max_size: usize,

    current_size: usize,
    
    entries: VecDeque<HeaderField>,

    // 

    // Reclaiming must decode
    // BTreeMap will be fine : need a way to hash it.

    // HashMap<>

    // entry_index: 
}

// TODO: Enqueue multiple max size changes together to ensure 

// SETTINGS_HEADER_TABLE_SIZE changes only apply when it is acknowledged.

// HEADER frames must be send in contiguous frames.

impl DynamicTable {
    pub fn new(max_size: usize) -> Self {
        DynamicTable { max_size, current_size: 0, entries: VecDeque::new() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn current_size(&self) -> usize {
        self.current_size
    }

    pub fn index(&self, idx: usize) -> Option<&HeaderField> {
        self.entries.get(idx)
    }

    pub fn insert(&mut self, header_field: HeaderField) {
        let new_entry_size = Self::entry_size(&header_field);

        if new_entry_size > self.max_size {
            self.entries.clear();
            return;
        }

        while self.current_size + new_entry_size > self.max_size {
            if let Some(entry) = self.entries.pop_back() {
                self.current_size -= Self::entry_size(&entry);
            } else {
                break;
            }
        }

        self.entries.push_front(header_field);
        self.current_size += new_entry_size;
    }

    // TODO: Need to be able to look up.

    pub fn resize(&mut self, new_size: usize) {
        // if new_size > self.ceiling_size {
        //     return Err(err_msg("New dynamic table size too large"));
        // }

        while self.current_size > new_size {
            if let Some(entry) = self.entries.pop_back() {
                self.current_size -= Self::entry_size(&entry);
            } else {
                panic!("Hit end of headers before current_size = 0");
            }
        }

        self.max_size = new_size;
    }

    // TODO: Also need to support changing the ceiling_size.

    fn entry_size(header_field: &HeaderField) -> usize {
        header_field.name.len() + header_field.value.len() + 32
    }

}

/*
It is not an error to
   attempt to add an entry that is larger than the maximum size; an
   attempt to add an entry larger than the maximum size causes the table
   to be emptied of all existing entries and results in an empty table.
*/

// 32 + a + b