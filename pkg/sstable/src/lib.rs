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
#[macro_use]
extern crate file;

mod arena;
pub mod db;
mod encoding;
pub mod memtable;
pub mod record_log;
// mod skip_list;
pub mod iterable;
pub mod log_writer;
pub mod table;

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

pub use db::{EmbeddedDB, EmbeddedDBOptions};
