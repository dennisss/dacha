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

use crate::db::EmbeddedDBOptions;
use common::errors::*;
use std::path::Path;

pub mod db;
pub mod encoding;
mod internal_key;
mod manifest;
mod memtable;
pub mod record_log;
// mod skip_list;
pub mod table;
mod write_batch;

/*
    At each level, we will store a Vec<File> that we will binary search for
    which

*/

/*
    We will have at most two MemTables
    - One being flushed to disk and one

    TODO: If a key was only ever in memory, we can delete it without
*/

// leveldb.BytewiseComparator

pub async fn open_db(path: &str) -> Result<()> {
    db::EmbeddedDB::open(&Path::new(path), EmbeddedDBOptions::default()).await?;

    //	let log = record_log::

    Ok(())
}
