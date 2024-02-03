use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{channel, lock};

use crate::proto::WatchResponse;

pub struct Watchers {
    state: Arc<AsyncMutex<WatchersState>>,
}

struct WatchersState {
    // TODO: Use a BTreeMap
    prefix_watchers: Vec<WatcherEntry>,

    last_id: usize,
}

struct WatcherEntry {
    key_prefix: Bytes,
    id: usize,
    // client_id: String,
    sender: channel::Sender<WatchResponse>,
}

impl Watchers {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AsyncMutex::new(WatchersState {
                prefix_watchers: vec![],
                last_id: 0,
            })),
        }
    }

    /// CANCEL SAFE
    pub async fn register(&self, prefix: &[u8]) -> WatcherRegistration {
        let mut state = self.state.lock().await.unwrap().read_exclusive();

        let id = state.last_id + 1;
        let (sender, receiver) = channel::unbounded();

        lock!(state <= state.upgrade(), {
            state.last_id = id;

            let entry = WatcherEntry {
                key_prefix: Bytes::from(prefix),
                id,
                sender,
            };

            // NOTE: These two lines must happen atomically to ensure that the entry is
            // always cleaned up.
            state.prefix_watchers.push(entry);
        });

        WatcherRegistration {
            state: self.state.clone(),
            id,
            receiver,
        }
    }

    pub async fn broadcast(&self, change: &WatchResponse) {
        let state = self.state.lock().await.unwrap().enter();
        for watcher in &state.prefix_watchers {
            let mut filtered_response = WatchResponse::default();
            for entry in change.entries() {
                if entry.key().starts_with(watcher.key_prefix.as_ref()) {
                    filtered_response.add_entries(entry.as_ref().clone());
                }
            }

            if !filtered_response.entries().is_empty() {
                // NOTE: To prevent blocking the write path, this must use an unbounded channel.
                let _ = watcher.sender.send(filtered_response).await;
            }
        }

        state.exit();
    }
}

pub struct WatcherRegistration {
    state: Arc<AsyncMutex<WatchersState>>,
    id: usize,
    receiver: channel::Receiver<WatchResponse>,
}

impl Drop for WatcherRegistration {
    fn drop(&mut self) {
        let state = self.state.clone();
        let id = self.id;
        executor::spawn(async move {
            let mut state = state.lock().await.unwrap().enter();
            for i in 0..state.prefix_watchers.len() {
                if state.prefix_watchers[i].id == id {
                    state.prefix_watchers.swap_remove(i);
                    break;
                }
            }

            state.exit();
        });
    }
}

impl WatcherRegistration {
    pub async fn recv(&self) -> Result<WatchResponse> {
        let v = self.receiver.recv().await?;
        Ok(v)
    }
}
