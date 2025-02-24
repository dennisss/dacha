use std::collections::HashMap;
use std::sync::Arc;

use base_error::*;
use common::bytes::Bytes;
use db_kv::*;
use db_table::db::*;
use db_table::query::*;
use db_table::table::*;
use executor::lock;
use executor::sync::AsyncMutex;
use file::LocalPath;
use protobuf::reflection::{Reflection, ReflectionMut};
use protobuf::{FieldNumber, Message, MessageReflection, StaticMessage, TypedFieldNumber};
use sstable::db::SnapshotIterator;
use sstable::db::WriteBatch;
use sstable::transactional::TransactionalEmbeddedDB;
use sstable::{db::SnapshotIteratorOptions, iterable::Iterable};
use sstable::{EmbeddedDB, EmbeddedDBOptions};

pub async fn create_db_instance(path: &LocalPath) -> Result<ProtobufDB> {
    let mut options = EmbeddedDBOptions::default();
    options.create_if_missing = true;
    options.error_if_exists = false;

    let db = TransactionalEmbeddedDB::open(path, options).await?;
    Ok(ProtobufDB::new(Arc::new(db)))
    }
