use common::async_std::channel;

pub struct Channel<T> {
    sender: channel::Sender<T>,
    receiver: channel::Receiver<T>,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        let (sender, receiver) = channel::bounded(1);
        Self { sender, receiver }
    }

    pub async fn try_send(&self, value: T) -> bool {
        self.sender.try_send(value).is_ok()
    }

    pub async fn send(&self, value: T) {
        let _ = self.sender.send(value).await;
    }

    pub async fn try_recv(&self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(v) => Some(v),
            _ => None,
        }
    }

    pub async fn recv(&self) -> T {
        self.receiver.recv().await.unwrap()
    }
}
