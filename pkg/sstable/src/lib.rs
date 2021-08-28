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

mod db;
pub mod encoding;
mod internal_key;
mod manifest;
mod memtable;
pub mod record_log;
// mod skip_list;
pub mod table;
mod write_batch;

use common::errors::*;
use std::path::Path;

/*
    At each level, we will store a Vec<File> that we will binary search for
    which

*/

/*
    We will have at most two MemTables
    - One being flushed to disk and one

    TODO: If a key was only ever in memory, we can delete it without
*/

pub use db::{EmbeddedDB, EmbeddedDBOptions};
