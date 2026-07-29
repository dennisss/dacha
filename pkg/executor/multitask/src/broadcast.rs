use std::sync::Arc;

use base_error::*;
use executor::sync::SyncMutex;
use executor::lock;
use executor::channel::spsc;
use executor::channel::error::SendError;

/// Channel which when published to, sends a copy of the published data to
/// all subscribers/receivers. 
#[derive(Default)]
pub struct BroadcastChannel<T> {
    shared: Arc<Shared<T>>
}

#[derive(Default)]
struct Shared<T> {
    state: SyncMutex<State<T>>
}

#[derive(Default)]
struct State<T> {
    last_subscriber_id: u64,
    subscribers: Vec<(u64, spsc::Sender<T>)>,
}

impl<T: Clone> BroadcastChannel<T> {
    /// Returns true if there is at least one subscriber.
    pub fn active(&self) -> bool {
        self.shared.state.apply(|s| {
            !s.subscribers.is_empty()
        }).unwrap()
    }

    /// Returns the number of subscribers that won't get the data because their
    /// receive queues are out of space.
    pub fn send(&self, entry: T) -> usize {
        self.shared.state.apply(move |state| {
            let mut num_bounced = 0;

            let mut i = 0;
            while i < state.subscribers.len() {

                match state.subscribers[i].1.try_send(entry.clone()) {
                    Ok(()) => {}
                    Err(err) => {
                        match err.error {
                            SendError::OutOfSpace => {
                                num_bounced += 1;
                            }
                            SendError::ReceiverDropped => {
                                state.subscribers.swap_remove(i);
                                continue;
                            }
                        }
                    }                    
                }

                i += 1;
            }

            num_bounced
        }).unwrap()
    }

    pub fn subscribe(&self, queue_size: usize) -> BroadcastChannelSubscriber<T> {
        let (sender, receiver) = spsc::bounded(queue_size);
 
        self.shared.state.apply(move |state| {
            let id = state.last_subscriber_id + 1;
            state.last_subscriber_id = id;

            state.subscribers.push((id, sender));

            BroadcastChannelSubscriber {
                shared: self.shared.clone(),
                id,
                receiver
            }
        }).unwrap()
    }
}

pub struct BroadcastChannelSubscriber<T> {
    shared: Arc<Shared<T>>,
    id: u64,
    receiver: spsc::Receiver<T>
}

impl<T> BroadcastChannelSubscriber<T> {
    pub async fn recv(&mut self) -> Result<T> {
        Ok(self.receiver.recv().await?)
    }

    pub fn try_recv(&mut self) -> Option<Result<T>> {
        if let Some(v) = self.receiver.try_recv() {
            Some(v.map_err(|e| e.into()))
        } else {
            None
        }
    }

    pub async fn wait(&mut self) {
        self.receiver.wait().await
    }
}

impl<T> Drop for BroadcastChannelSubscriber<T> {
    fn drop(&mut self) {
        self.shared.state.apply(|state| {
            // TODO: Speed this up by limiting to just one removal
            state.subscribers.retain(|(id, _)| *id != self.id)
        });
    }
}
