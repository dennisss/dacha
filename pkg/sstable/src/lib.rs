#[macro_use]
extern crate parsing;
#[macro_use]
extern crate arrayref;
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
use common::async_std::fs::File;
use common::async_std::io::{Read, Seek, SeekFrom, Write};
use common::async_std::path::Path;
use common::bytes::Bytes;
use common::errors::*;
use crypto::hasher::Hasher;
use parsing::*;
use protobuf::wire::parse_varint;
use std::cmp::min;

mod block;
mod block_builder;
pub mod bloom;
mod comparator;
mod comparator_context;
pub mod db;
pub mod encoding;
pub mod filter_block;
pub mod filter_block_builder;
mod internal_key;
mod manifest;
mod memtable;
mod record_log;
pub mod table;
pub mod table_builder;
mod table_properties;
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
