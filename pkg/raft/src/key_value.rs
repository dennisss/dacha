use std::collections::HashMap;
use std::io::Read;
use std::time::SystemTime;

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::Mutex;
use protobuf::{Message, StaticMessage};
use raft::atomic::*;
use raft::proto::*;
use raft::server::state_machine::*;

pub struct KeyValueReturn {
    pub success: bool,
}

pub struct KeyValueData {
    pub version: LogIndex,
    pub expires: Option<SystemTime>,

    // XXX: May also be of different types (AKA mainly could be either a blob, set, or list in
    // redis land)
    pub value: Bytes,
}

/*
    Scaling Redis performance
    - Mainly would be based on the splitting of operations across multiple systems
        - Naturally if we support parititioning, then we can support
    - Mixing consistency levels
        - Easiest to do this over specific key ranges as mixing consistency levels will end up downgrading the gurantees to the lowest consistency level available

*/

struct State {
    last_applied: LogIndex,
    data: HashMap<Vec<u8>, Bytes>,

    /// Reference to the most recent snapshot taken of the machine
    snapshot: Option<StateMachineSnapshot>,

    /// Optionally where new snapshots will be stored on disk
    snapshot_file: Option<BlobFile>,
}

/// A simple key-value state machine implementation that provides most redis
/// style functionality including atomic (multi-)key operations and transactions
/// NOTE: This does not
pub struct MemoryKVStateMachine {
    // Better to also hold on to a version and possibly
    data: Mutex<HashMap<Vec<u8>, Bytes>>,
}

impl MemoryKVStateMachine {
    /*
    pub fn from_file(path: &Path) -> Result<(LogIndex, MemoryKVStateMachine)> {

        let builder = BlobFile::builder(path)?;
        if builder.exists() {
            let (file, data) = builder.open()?;

            let snapshot = unmarshal::<KVStateMachineSnapshot>()?;

            let machine = MemoryKVStateMachine {
                data: snapshot.data
            };



            // Using the snapshot we will
            // Interestingly it is sometimes useful to get a read index on the file later on

        }
        else {
            // Store an initial empty snapsoht

            let file = builder.create(KVStateMachineSnapshot {
                last_applied: 0,

            })?;

            let machine = MemoryKVStateMachine::new();

            // Would be super useful to be able to re-read a file (I don't want to be forced to read any entire snapshot from disk)
                // Usuaully most of the snapshot will already be in RAM

            // A better

            Ok((0, machine))

        }


    }
    */

    pub fn new() -> MemoryKVStateMachine {
        MemoryKVStateMachine {
            data: Mutex::new(HashMap::new()),
        }
    }

    /// Very simple, non-linearizable read operation
    pub async fn get(&self, key: &[u8]) -> Option<Bytes> {
        let data = self.data.lock().await;

        // TODO: Probably inefficient (probably better to return an Arc)
        data.get(key).map(|v| v.clone())
    }
}

#[async_trait]
impl StateMachine<KeyValueReturn> for MemoryKVStateMachine {
    // XXX: It would be useful to have a time and an index just for the sake of
    // versioning of it
    async fn apply(&self, index: LogIndex, data: &[u8]) -> Result<KeyValueReturn> {
        let ret = KeyValueOperation::parse(data)?;

        let mut map = self.data.lock().await;

        // Could be split into a check phase and a run phase
        // Thus we can maintain transactions without lock

        match ret.type_case() {
            KeyValueOperationTypeCase::Set(op) => {
                map.insert(op.key().to_owned(), Bytes::from(op.value()));

                Ok(KeyValueReturn { success: true })
            }
            KeyValueOperationTypeCase::Delete(op) => {
                let old = map.remove(op.key());
                Ok(KeyValueReturn {
                    success: old.is_some(),
                })
            }
            KeyValueOperationTypeCase::Unknown => Err(err_msg("Unknown key-value operation")),
        }
    }

    async fn last_flushed(&self) -> LogIndex {
        0.into()
    }

    async fn wait_for_flush(&self) {
        // The state machine never snapshots itself.
        common::futures::future::pending().await
    }

    async fn snapshot(&self) -> Option<StateMachineSnapshot> {
        // For now just copy the

        // Possibly consider it to have a zero-byte snapshot that generates an empty
        // state machine Alternatively, we can just assume that all snapshots
        // will only be available in memory Complicates in readin them back
        // without a mmemory lock though
        None
    }

    async fn restore(&self, data: StateMachineSnapshot) -> Result<()> {
        // A snapshot should not have been generatable
        Ok(())
    }
}

/// Basically a MemoryKVStateMachine backed by a single file
pub struct PersistentKVStateMachine {}

// Will end up being basically another passthrough system
