//! This file contains utilities for reading/writing from the node local
//! database used by a node to remember what has done in the past.

use builder::proto::BundleBlobSpec;
use common::errors::*;
use container_proto::cluster::*;
use db_table::db::ProtobufDB;
use db_table::{define_singleton_table, query, query_one, table::*};
use protobuf::{Message, StaticMessage};

use crate::proto::{Labels, WorkerEvent, WorkerMetadata};

const WORKERS_TABLE_ID: u64 = 11;
const NODE_ID_TABLE_ID: u64 = 12;
const BLOBS_TABLE_ID: u64 = 13;
const EVENTS_TABLE_ID: u64 = 14;
const EVENTS_TIMESTAMP_ID: u64 = 15;
const NODE_LABELS_ID: u64 = 16;

/// Table that contains only the WorkerMetadata for workers that are assigned to
/// the current node.
pub struct LocalWorkerMetadataTable {}

impl ProtobufTableTag for LocalWorkerMetadataTable {
    type Message = WorkerMetadata;

    fn table_id() -> u32 {
        11
    }

    fn table_name() -> &'static str {
        "LocalWorkerMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[
                    WorkerMetadata::SPEC_FIELD_NUM_RAW,
                    WorkerSpec::NAME_FIELD_NUM_RAW,
                ],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}

pub async fn delete_worker(db: &ProtobufDB, worker_name: &str) -> Result<()> {
    let mut entry = WorkerMetadata::default();
    entry.spec_mut().set_name(worker_name);
    db.remove::<LocalWorkerMetadataTable>(&entry).await
}

/// Table used to keep track of all the blobs that are replicated locally on
/// this node.
pub struct LocalBundleBlobSpecTable {}

impl ProtobufTableTag for LocalBundleBlobSpecTable {
    type Message = BundleBlobSpec;

    fn table_id() -> u32 {
        13
    }

    fn table_name() -> &'static str {
        "LocalBundleBlobSpec"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[BundleBlobSpec::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}

pub async fn delete_blob_spec(db: &ProtobufDB, blob_id: &str) -> Result<()> {
    let mut msg = BundleBlobSpec::default();
    msg.set_id(blob_id);
    db.remove::<LocalBundleBlobSpecTable>(&msg).await?;
    Ok(())
}

pub async fn get_events_timestamp(db: &ProtobufDB) -> Result<Option<u64>> {
    let entry = query_one!(db, WorkerEventLatestTimestampTable, "TRUE");

    if let Some(entry) = entry {
        return Ok(Some(entry.timestamp()));
    }

    Ok(None)
}

pub struct WorkerEventTable {}

impl ProtobufTableTag for WorkerEventTable {
    type Message = WorkerEvent;

    fn table_id() -> u32 {
        14
    }

    fn table_name() -> &'static str {
        "WorkerEvent"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[
                ProtobufKeyField {
                    path: &[WorkerEvent::WORKER_NAME_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                },
                ProtobufKeyField {
                    path: &[WorkerEvent::TIMESTAMP_FIELD_NUM_RAW],
                    direction: Direction::Descending,
                    fixed_size: false,
                },
            ],
        }]
    }
}

define_singleton_table!(WorkerEventLatestTimestampTable {
    message: WorkerEventLatestTimestamp,
    table_id: 15,
    table_name: "WorkerEventLatestTimestamp"
});

// NOTE: This assumes that the user has already ensured that the timestamp in
// the event is monotonic.
pub async fn put_worker_event(db: &ProtobufDB, event: &WorkerEvent) -> Result<()> {
    let mut txn = db.new_transaction().await?;
    txn.put::<WorkerEventTable>(event).await?;

    // TODO: Have a read lock for this to prevent overwriting another slightly
    // larger timestamp.
    let mut time = WorkerEventLatestTimestamp::default();
    time.set_timestamp(event.timestamp());
    txn.put::<WorkerEventLatestTimestampTable>(&time).await?;

    txn.commit().await?;

    Ok(())
}

pub async fn get_worker_events(db: &ProtobufDB, worker_name: &str) -> Result<Vec<WorkerEvent>> {
    let out = query!(db, WorkerEventTable, "worker_name = ?", worker_name);
    Ok(out)
}

pub struct WorkerRuntimeMetadataTable {}

impl ProtobufTableTag for WorkerRuntimeMetadataTable {
    type Message = WorkerRuntimeMetadata;

    fn table_id() -> u32 {
        19
    }

    fn table_name() -> &'static str {
        "WorkerRuntimeMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[WorkerRuntimeMetadata::WORKER_NAME_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}
