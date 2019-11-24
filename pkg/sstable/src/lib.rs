#[macro_use] extern crate parsing;
#[macro_use] extern crate arrayref;
#[macro_use] extern crate common;
#[macro_use] extern crate reflection;
extern crate math;
extern crate async_std;
extern crate protobuf;
extern crate compression;
extern crate crypto;

use common::errors::*;
use parsing::*;
use protobuf::wire::{parse_varint};
use async_std::io::{Read, Seek, Write, SeekFrom};
use std::cmp::min;
use bytes::Bytes;
use async_std::fs::File;
use async_std::path::Path;
use crypto::hasher::Hasher;
use crate::db::EmbeddedDBOptions;

pub mod encoding;
mod comparator;
mod comparator_context;
mod internal_key;
mod block;
mod block_builder;
mod table_properties;
mod manifest;
mod record_log;
mod write_batch;
mod memtable;
pub mod table;
pub mod db;
pub mod table_builder;
pub mod filter_block;
pub mod filter_block_builder;
pub mod bloom;

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