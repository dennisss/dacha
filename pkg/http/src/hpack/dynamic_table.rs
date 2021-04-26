use std::collections::VecDeque;

use crate::hpack::header_field::HeaderField;

// TODO: For the purposes of encoding, this should be indexed by name.
struct DynamicTable {
    // /// Absolute maximum size of the table as indicated by the protocol using HPACK.
    // ceiling_size: usize,
    
    /// Current maximum size of the table. When the size of the table exceeds this value,
    /// entries will be evicted. 
    reserved_size: usize,

    current_size: usize,
    
    entries: VecDeque<HeaderField>,

    // entry_index: 
}

// TODO: Enqueue multiple max size changes together to ensure 

// SETTINGS_HEADER_TABLE_SIZE changes only apply when it is acknowledged.

// HEADER frames must be send in contiguous frames.

impl DynamicTable {

    pub fn insert(&mut self, header_field: HeaderField) {
        let new_entry_size = Self::entry_size(&header_field);

        if new_entry_size > self.reserved_size {
            self.entries.clear();
            return;
        }

        while self.current_size + new_entry_size > self.reserved_size {
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

        self.reserved_size = new_size;
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