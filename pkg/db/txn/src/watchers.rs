use std::sync::Arc;
use std::collections::HashMap;

use common::bytes::Bytes;
use base_error::*;
use executor::sync::AsyncMutex;
use executor::lock;
use executor::channel::spsc;
use db_txn_proto::db::txn::WatchResponse;
use db_kv::KeyRanges;
use db_table::key_utils::single_key_range;
use common::hash::FastHasherBuilder;

pub struct Watchers {
    state: Arc<AsyncMutex<WatchersState>>,
}

struct WatchersState {
    // TODO: Switch this to using a slab with a smaller id integer type.
    watchers: HashMap<u64, WatcherEntry, FastHasherBuilder>,
    watched_ranges: KeyRanges<WatchedRange>,
    last_id: u64,
}

#[derive(Clone, Default, Debug)]
struct WatchedRange {
    watcher_ids: Vec<u64>,
}

struct WatcherEntry {    
    // TODO: Need to detect when this has run out of space. If the client only cares about knowing if a change happened, then that would be easier.
    sender: spsc::Sender<WatchResponse>,
}

impl Watchers {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AsyncMutex::new(WatchersState {
                watchers: HashMap::default(),
                watched_ranges: KeyRanges::new(),
                last_id: 0,
            })),
        }
    }

    /// CANCEL SAFE
    pub async fn register(&self, start_key: &[u8], end_key: &[u8]) -> WatcherRegistration {
        let start_key = Bytes::from(start_key);
        let end_key = Bytes::from(end_key);

        let mut state = self.state.lock().await.unwrap().read_exclusive();

        let id = state.last_id + 1;

        // TODO: Needs to be bounded.
        let (sender, receiver) = spsc::bounded(4);

        // NOTE: These lines must happen atomically to ensure that the entry is
        // always cleaned up.
        lock!(state <= state.upgrade(), {
            state.last_id = id;

            state.watchers.insert(id, WatcherEntry {
                sender,
            });

            state.watched_ranges.range_mut(start_key.clone(), end_key.clone(), |range| {
                range.watcher_ids.push(id);
                true
            });

            WatcherRegistration {
                state: self.state.clone(),
                id,
                start_key,
                end_key,
                receiver,
            }
        })
    }

    pub async fn broadcast(&self, change: &WatchResponse) {
        let mut state = self.state.lock().await.unwrap().enter();

        // TODO: Re-use memory across broadcast calls for this.
        let mut responses = HashMap::<u64, WatchResponse, FastHasherBuilder>::default();

        for entry in change.entries() {

            let (start_key, end_key) = single_key_range(entry.key()); 

            let mut num_times = 0;

            // NOTE: This should only ever explore a single range.
            state.watched_ranges.range(&start_key, &end_key, |range| {
                for watcher_id in range.watcher_ids.iter().cloned() {
                    responses.entry(watcher_id).or_default().add_entries(entry.as_ref().clone());
                }
            });
        }

        for (watcher_id, response) in responses.drain() {
            let watcher = state.watchers.get_mut(&watcher_id).unwrap();

            // TODO: Must handle rejections and notify the client that data was skipped.
            let _ = watcher.sender.try_send(response);
        }

        state.exit();
    }
}

pub struct WatcherRegistration {
    state: Arc<AsyncMutex<WatchersState>>,
    id: u64,
    start_key: Bytes,
    end_key: Bytes,
    receiver: spsc::Receiver<WatchResponse>,
}

impl Drop for WatcherRegistration {
    fn drop(&mut self) {
        let state = self.state.clone();
        let start_key = self.start_key.clone();
        let end_key = self.end_key.clone();
        let id = self.id;
        executor::spawn(async move {
            let mut state = state.lock().await.unwrap().enter();

            state.watchers.remove(&id);

            state.watched_ranges.range_mut(start_key, end_key, |range| {
                for i in 0..range.watcher_ids.len() {
                    if range.watcher_ids[i] == id {
                        range.watcher_ids.swap_remove(i);
                        break;
                    }
                }

                range.watcher_ids.len() > 0
            });

            state.exit();
        });
    }
}

impl WatcherRegistration {
    pub async fn recv(&mut self) -> Result<WatchResponse> {
        let v = self.receiver.recv().await?;
        Ok(v)
    }
}
