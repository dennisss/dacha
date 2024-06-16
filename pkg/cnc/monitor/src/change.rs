use std::collections::HashMap;
use std::sync::Arc;

use cnc_monitor_proto::cnc::EntityType;
use common::hash::FastHasherBuilder;
use executor::{channel, child_task::ChildTask, sync::SyncMutex};

#[derive(Clone)]
pub struct ChangeEvent {
    pub entity_type: EntityType,
    pub id: Option<u64>,
    pub verbose: bool,
}

impl ChangeEvent {
    pub fn new(entity_type: EntityType, id: Option<u64>, verbose: bool) -> Self {
        Self {
            entity_type,
            id,
            verbose,
        }
    }
}

impl ChangeEvent {
    // Whether the current event is a superset of 'event'
    fn matches(&self, event: &ChangeEvent) -> bool {
        if self.entity_type != event.entity_type {
            return false;
        }

        if !self.verbose && event.verbose {
            return false;
        }

        if let Some(id) = &self.id {
            if let Some(id2) = &event.id {
                if *id != *id2 {
                    return false;
                }
            }
        }

        true
    }
}

pub struct ChangeDistributer {
    shared: Arc<Shared>,
    task: ChildTask,
}

struct Shared {
    sender: channel::Sender<ChangeEvent>,
    state: SyncMutex<State>,
}

#[derive(Default)]
struct State {
    subscribers: HashMap<u64, SubscriberEntry, FastHasherBuilder>,
    next_subscriber_id: u64,
}

struct SubscriberEntry {
    filter: ChangeEvent,

    // Note that this has only one slot.
    // TODO: Change this into an spsc or a simpler event notifier implementation.
    sender: channel::Sender<()>,
}

impl ChangeDistributer {
    pub fn create() -> Self {
        let (sender, receiver) = channel::unbounded();

        let shared = Arc::new(Shared {
            sender,
            state: SyncMutex::default(),
        });

        let task = ChildTask::spawn(Self::distributor_thread(shared.clone(), receiver));

        Self { shared, task }
    }

    async fn distributor_thread(shared: Arc<Shared>, receiver: channel::Receiver<ChangeEvent>) {
        loop {
            let event = receiver.recv().await.unwrap();

            shared.state.apply(|state| {
                for sub in state.subscribers.values() {
                    if !sub.filter.matches(&event) {
                        continue;
                    }

                    let _ = sub.sender.try_send(());
                }
            });
        }
    }

    pub fn publisher(&self) -> ChangePublisher {
        ChangePublisher {
            sender: self.shared.sender.clone(),
        }
    }

    pub fn subscribe(&self, filter: ChangeEvent) -> ChangeReciever {
        let (sender, receiver) = channel::bounded(1);

        let id = self
            .shared
            .state
            .apply(|state| {
                let id = state.next_subscriber_id;
                state.next_subscriber_id += 1;
                state
                    .subscribers
                    .insert(id, SubscriberEntry { filter, sender });
                id
            })
            .unwrap();

        ChangeReciever {
            id,
            receiver,
            shared: self.shared.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ChangePublisher {
    sender: channel::Sender<ChangeEvent>,
}

impl ChangePublisher {
    pub fn publish(&self, event: ChangeEvent) {
        let _ = self.sender.try_send(event);
    }
}

/// Dropping of this will signal a loss of interest in the changes.
pub struct ChangeReciever {
    id: u64,
    receiver: channel::Receiver<()>,
    shared: Arc<Shared>,
}

impl Drop for ChangeReciever {
    fn drop(&mut self) {
        let _ = self.shared.state.apply(|state| {
            state.subscribers.remove(&self.id);
        });
    }
}

impl ChangeReciever {
    // Wait until
    pub async fn wait(&self) {
        let _ = self.receiver.recv().await;
    }
}
