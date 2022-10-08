// This file contains utilities for reading/writing from the node local database
// used by a node to remember what has done in the past.

use builder::proto::bundle::BlobSpec;
use common::errors::*;
use datastore::key_encoding::KeyEncoder;
use protobuf::{Message, StaticMessage};
use sstable::db::WriteBatch;
use sstable::iterable::Iterable;
use sstable::EmbeddedDB;

use crate::proto::meta::WorkerMetadata;
use crate::proto::worker_event::WorkerEvent;

const WORKERS_TABLE_ID: u64 = 11;
const NODE_ID_TABLE_ID: u64 = 12;
const BLOBS_TABLE_ID: u64 = 13;
const EVENTS_TABLE_ID: u64 = 14;
const EVENTS_TIMESTAMP_ID: u64 = 15;

pub async fn list_workers(db: &EmbeddedDB) -> Result<Vec<WorkerMetadata>> {
    let mut start_key = vec![];
    KeyEncoder::encode_varuint(WORKERS_TABLE_ID, false, &mut start_key);

    let mut iter = db.snapshot().await.iter().await?;
    iter.seek(&start_key).await?;

    let mut workers = vec![];

    while let Some(entry) = iter.next().await? {
        let (table_id, _) = KeyEncoder::decode_varuint(&entry.key, false)?;
        if table_id != WORKERS_TABLE_ID {
            break;
        }

        // TODO: Pull the name out of the key.
        if let Some(value) = entry.value {
            workers.push(WorkerMetadata::parse(&value)?);
        }
    }

    Ok(workers)
}

pub async fn delete_worker(db: &EmbeddedDB, worker_name: &str) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(WORKERS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(worker_name.as_bytes(), &mut key);

    db.delete(&key).await
}

pub async fn put_worker(db: &EmbeddedDB, worker: &WorkerMetadata) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(WORKERS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(worker.spec().name().as_bytes(), &mut key);

    let value = worker.serialize()?;

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

pub async fn get_events_timestamp(db: &EmbeddedDB) -> Result<Option<u64>> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(EVENTS_TIMESTAMP_ID, false, &mut key);

    let value = db.get(&key).await?;

    if let Some(value) = value {
        if value.len() != 8 {
            return Err(err_msg("Invalid event timestamp length"));
        }

        return Ok(Some(u64::from_le_bytes(*array_ref![value, 0, 8])));
    }

    Ok(None)
}

// NOTE: This assumes that the user has already ensured that the timestamp in
// the event is monotonic.
pub async fn put_worker_event(db: &EmbeddedDB, event: &WorkerEvent) -> Result<()> {
    let mut batch = WriteBatch::new();

    {
        let mut key = vec![];
        KeyEncoder::encode_varuint(EVENTS_TABLE_ID, false, &mut key);
        // TODO: Don't store these in the value given that they are present in the key.
        KeyEncoder::encode_bytes(event.worker_name().as_bytes(), &mut key);
        KeyEncoder::encode_varuint(event.timestamp(), true, &mut key);

        let value = event.serialize()?;
        batch.put(&key, &value);
    }

    {
        let mut time_key = vec![];
        KeyEncoder::encode_varuint(EVENTS_TIMESTAMP_ID, false, &mut time_key);

        let value = (event.timestamp() as u64).to_le_bytes();
        batch.put(&time_key, &value);
    }

    db.write(&mut batch).await?;

    Ok(())
}

pub async fn get_worker_events(db: &EmbeddedDB, worker_name: &str) -> Result<Vec<WorkerEvent>> {
    let mut start_key = vec![];
    KeyEncoder::encode_varuint(EVENTS_TABLE_ID, false, &mut start_key);
    KeyEncoder::encode_bytes(worker_name.as_bytes(), &mut start_key);

    let mut iter = db.snapshot().await.iter().await?;
    iter.seek(&start_key).await?;

    let mut out = vec![];
    while let Some(entry) = iter.next().await? {
        if !entry.key.starts_with(&start_key.as_ref()) {
            break;
        }

        if let Some(value) = entry.value {
            out.push(WorkerEvent::parse(&value)?);
        }
    }

    Ok(out)
}
