use common::errors::*;
use datastore::key_encoding::KeyEncoder;
use protobuf::Message;
use sstable::iterable::Iterable;
use sstable::EmbeddedDB;

use crate::proto::task::TaskSpec;

const TASKS_TABLE_ID: u64 = 11;

pub async fn list_tasks(db: &EmbeddedDB) -> Result<Vec<TaskSpec>> {
    let mut start_key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut start_key);

    let mut iter = db.snapshot().await.iter().await;
    iter.seek(&start_key).await?;

    let mut tasks = vec![];

    while let Some(entry) = iter.next().await? {
        let (table_id, _) = KeyEncoder::decode_varuint(&entry.key, false)?;
        if table_id != TASKS_TABLE_ID {
            break;
        }

        // TODO: Pull the name out of the key.
        tasks.push(TaskSpec::parse(&entry.value)?);
    }

    Ok(tasks)
}

pub async fn delete_task(db: &EmbeddedDB, task: &TaskSpec) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(task.name().as_bytes(), &mut key);

    db.delete(&key).await
}

pub async fn put_task(db: &EmbeddedDB, task: &TaskSpec) -> Result<()> {
    let mut key = vec![];
    KeyEncoder::encode_varuint(TASKS_TABLE_ID, false, &mut key);
    KeyEncoder::encode_bytes(task.name().as_bytes(), &mut key);

    let value = task.serialize()?;

    db.set(&key, &value).await
}
