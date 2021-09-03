use std::sync::Arc;

pub struct ArenaSlice {
    buffer: Arc<Vec<u8>>,
    slice_start: usize,
    slice_length: usize,
}

/*
Need to track amount of memory in use and

*/
