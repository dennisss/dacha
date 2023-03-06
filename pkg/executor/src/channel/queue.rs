use alloc::vec::Vec;

use crate::channel;

pub struct ConcurrentQueue<T> {
    sender: channel::Sender<T>,
    receiver: channel::Receiver<T>,
}

impl<T> ConcurrentQueue<T> {
    pub fn unbounded() -> Self {
        let (sender, receiver) = channel::unbounded();
        Self { sender, receiver }
    }

    pub async fn push_back(&self, value: T) {
        let _ = self.sender.send(value).await;
    }

    pub async fn pop_front(&self) -> T {
        self.receiver.recv().await.unwrap()
    }
}

impl<T> From<Vec<T>> for ConcurrentQueue<T> {
    fn from(value: Vec<T>) -> Self {
        let mut inst = ConcurrentQueue::unbounded();
        for v in value {
            inst.sender.try_send(v).unwrap();
        }

        inst
    }
}
