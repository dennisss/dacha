// This file contains utilities for reading/writing from the node local database
// used by a node to remember what has done in the past.

use common::errors::*;
use datastore::key_encoding::KeyEncoder;
use protobuf::Message;
use sstable::iterable::Iterable;
use sstable::EmbeddedDB;

use crate::proto::blob::BlobSpec;
use crate::proto::meta::TaskMetadata;

const TASKS_TABLE_ID: u64 = 11;
const NODE_ID_TABLE_ID: u64 = 12;
const BLOBS_TABLE_ID: u64 = 13;

pub async fn list_tasks(db: &EmbeddedDB) -> Result<Vec<TaskMetadata>> {
    let mut start_key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut start_key);

    let mut iter = db.snapshot().await.iter().await?;
    iter.seek(&start_key).await?;

    let mut tasks = vec![];

    while let Some(entry) = iter.next().await? {
        let (table_id, _) = KeyEncoder::decode_varuint(&entry.key, false)?;
        if table_id != TASKS_TABLE_ID {
            break;
        }

        // TODO: Pull the name out of the key.
        if let Some(value) = entry.value {
            tasks.push(TaskMetadata::parse(&value)?);
        }
    }

    Ok(tasks)
}

pub async fn delete_task(db: &EmbeddedDB, task: &TaskMetadata) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(task.spec().name().as_bytes(), &mut key);

    db.delete(&key).await
}

pub async fn put_task(db: &EmbeddedDB, task: &TaskMetadata) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(task.spec().name().as_bytes(), &mut key);

    let value = task.serialize()?;

    db.set(&key, &value).await
}

pub async fn get_saved_node_id(db: &EmbeddedDB) -> Result<Option<u64>> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(NODE_ID_TABLE_ID, false, &mut key);

    let value = db.get(&key).await?;

    if let Some(value) = value {
        if value.len() != 8 {
            return Err(err_msg("Invalid node id length"));
        }

        return Ok(Some(u64::from_le_bytes(*array_ref![value, 0, 8])));
    }

    Ok(None)
}

pub async fn set_saved_node_id(db: &EmbeddedDB, id: u64) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(NODE_ID_TABLE_ID, false, &mut key);

    let value = id.to_le_bytes();

    db.set(&key, &value).await
}

pub async fn put_blob_spec(db: &EmbeddedDB, spec: BlobSpec) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(BLOBS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(spec.id().as_bytes(), &mut key);

    let value = spec.serialize()?;
    db.set(&key, &value).await?;
    Ok(())
}

pub async fn delete_blob_spec(db: &EmbeddedDB, blob_id: &str) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(BLOBS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(blob_id.as_bytes(), &mut key);

    db.delete(&key).await?;
    Ok(())
}

pub async fn get_blob_specs(db: &EmbeddedDB) -> Result<Vec<BlobSpec>> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(BLOBS_TABLE_ID, false, &mut key);

    let mut iter = db.snapshot().await.iter().await?;
    iter.seek(&key).await?;

    let mut out = vec![];
    while let Some(entry) = iter.next().await? {
        if !entry.key.starts_with(key.as_ref()) {
            break;
        }

        if let Some(value) = entry.value {
            out.push(BlobSpec::parse(&value)?);
        }
    }

    Ok(out)
}
