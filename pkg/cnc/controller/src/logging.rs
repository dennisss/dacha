use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::sync::SyncMutex;
use executor::channel::spsc;
use executor::channel::error::SendError;
use cnc_controller_proto::cnc::LogEntry;


// TODO: replace with the BroadcastChannel

#[derive(Default)]
pub struct LoggingChannel {
    shared: Arc<Shared>
}

#[derive(Default)]
struct Shared {
    state: SyncMutex<State>
}

#[derive(Default)]
struct State {
    last_subscriber_id: u64,
    subscribers: Vec<(u64, spsc::Sender<Arc<LogEntry>>)>,
}

impl LoggingChannel {

    pub fn active(&self) -> bool {
        self.shared.state.apply(|s| {
            !s.subscribers.is_empty()
        }).unwrap()
    }

    pub fn send(&self, entry: LogEntry) {
        let entry = Arc::new(entry);

        self.shared.state.apply(move |state| {

            let mut i = 0;
            while i < state.subscribers.len() {

                match state.subscribers[i].1.try_send(entry.clone()) {
                    Ok(()) => {}
                    Err(err) => {
                        match err.error {
                            SendError::OutOfSpace => {
                                eprintln!("Log entry rejected (no space)");
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
        }).unwrap();
    }

    pub fn subscribe(&self) -> LoggingChannelSubscriber {
        let (sender, receiver) = spsc::bounded(1024);
 
        self.shared.state.apply(move |state| {
            let id = state.last_subscriber_id + 1;
            state.last_subscriber_id = id;

            state.subscribers.push((id, sender));

            LoggingChannelSubscriber {
                shared: self.shared.clone(),
                id,
                receiver
            }
        }).unwrap()
    }
}

// TODO: Clean up on drop.
pub struct LoggingChannelSubscriber {
    shared: Arc<Shared>,
    id: u64,
    receiver: spsc::Receiver<Arc<LogEntry>>
}

impl LoggingChannelSubscriber {
    pub async fn recv(&mut self) -> Result<Arc<LogEntry>> {
        Ok(self.receiver.recv().await?)
    }
}

