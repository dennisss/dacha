use std::collections::VecDeque;

use executor::channel;
use executor::sync::AsyncVariable;

// TODO: Make use of this.

pub struct SerialSendBuffer {
    pending_send: AsyncVariable<VecDeque<Entry>>,
}

struct Entry {
    callback: channel::oneshot::Sender<Option<String>>,
    command: String,
}
