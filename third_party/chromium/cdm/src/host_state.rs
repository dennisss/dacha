use std::collections::HashMap;

use base_error::*;
use executor::channel::oneshot;

use crate::bindings::KeyStatus;

pub struct HostState {
    // TODO: Make private
    pub init_sender: Option<oneshot::Sender<bool>>,
    next_promise_id: u32,
    promises: HashMap<u32, oneshot::Sender<Result<PromiseValue>>>,
}

impl HostState {
    pub fn new(init_sender: oneshot::Sender<bool>) -> Self {
        Self {
            init_sender: Some(init_sender),
            next_promise_id: 1,
            promises: HashMap::new(),
        }
    }

    pub fn new_promise(&mut self) -> Promise {
        let id = self.next_promise_id;
        self.next_promise_id += 1;

        let (sender, receiver) = oneshot::channel();
        self.promises.insert(id, sender);

        Promise { id, receiver }
    }

    pub fn resolve_promise(&mut self, promise_id: u32, result: Result<PromiseValue>) {
        let sender = match self.promises.remove(&promise_id) {
            Some(v) => v,
            None => {
                eprintln!("[CDM] No live promise with id: {}", promise_id);
                return;
            }
        };

        let _ = sender.send(result);
    }
}

pub enum PromiseValue {
    NewSession(String),
    Empty,
    KeyStatus(KeyStatus),
}

pub struct Promise {
    id: u32,
    receiver: oneshot::Receiver<Result<PromiseValue>>,
}

impl Promise {
    pub fn id(&self) -> u32 {
        self.id
    }

    async fn wait(self) -> Result<PromiseValue> {
        self.receiver
            .recv()
            .await
            .map_err(|()| err_msg("CDM cancelled operation"))?
    }

    pub async fn wait_empty(self) -> Result<()> {
        match self.wait().await? {
            PromiseValue::Empty => Ok(()),
            _ => Err(err_msg("CDM returned wrong type returned from promise")),
        }
    }

    pub async fn wait_new_session(self) -> Result<String> {
        match self.wait().await? {
            PromiseValue::NewSession(s) => Ok(s),
            _ => Err(err_msg("CDM returned wrong type returned from promise")),
        }
    }
}
