use std::sync::Arc;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;

use crate::proto::key_value::KeyValueEntry;

pub struct Watchers {
    state: Arc<Mutex<WatchersState>>,
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
    sender: channel::Sender<KeyValueEntry>,
}

impl Watchers {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(WatchersState {
                prefix_watchers: vec![],
                last_id: 0,
            })),
        }
    }

    pub async fn register(&self, prefix: &[u8]) -> WatcherRegistration {
        let mut state = self.state.lock().await;

        let id = state.last_id + 1;
        state.last_id = id;

        let (sender, receiver) = channel::unbounded();

        let entry = WatcherEntry {
            key_prefix: Bytes::from(prefix),
            id,
            sender,
        };

        // NOTE: These two lines must happen atomically to ensure that the entry is
        // always cleaned up.
        state.prefix_watchers.push(entry);
        WatcherRegistration {
            state: self.state.clone(),
            id,
            receiver,
        }
    }

    pub async fn broadcast(&self, entry: &KeyValueEntry) {
        let state = self.state.lock().await;
        for watcher in &state.prefix_watchers {
            // if &watcher.client_id == client_id {
            //     continue;
            // }

            if entry.key().starts_with(watcher.key_prefix.as_ref()) {
                // NOTE: To prevent blocking the write path, this must use an unbounded channel.
                let _ = watcher.sender.send(entry.clone()).await;
            }
        }
    }
}

pub struct WatcherRegistration {
    state: Arc<Mutex<WatchersState>>,
    id: usize,
    receiver: channel::Receiver<KeyValueEntry>,
}

impl Drop for WatcherRegistration {
    fn drop(&mut self) {
        let state = self.state.clone();
        let id = self.id;
        task::spawn(async move {
            let mut state = state.lock().await;
            for i in 0..state.prefix_watchers.len() {
                if state.prefix_watchers[i].id == id {
                    state.prefix_watchers.swap_remove(i);
                    break;
                }
            }
        });
    }
}

impl WatcherRegistration {
    pub async fn recv(&self) -> Result<KeyValueEntry> {
        let v = self.receiver.recv().await?;
        Ok(v)
    }
}