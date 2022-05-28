extern crate alloc;
extern crate core;

#[macro_use]
extern crate parsing;
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate crypto;
extern crate math;
extern crate protobuf;
extern crate reflection;

mod arena;
pub mod db;
mod encoding;
pub mod memtable;
pub mod record_log;
// mod skip_list;
pub mod iterable;
pub mod table;

/*
    At each level, we will store a Vec<File> that we will binary search for
    which

*/

/*
Were will be use a SyncedFile?
- Misc Files
    - CURRENT
    - IDENTITY
- RecordWriter
    - Log
    - Manifest
- SSTableBuilder
    -

- Main challenges:
    - If we need to write many files (e.g. having multiple output files during a compaction, we should prefer to flush the directory just once.
*/

/*
    We will have at most two MemTables
    - One being flushed to disk and one

    TODO: If a key was only ever in memory, we can delete it without
*/

pub use db::{EmbeddedDB, EmbeddedDBOptions};
