use common::errors::*;
use common::bytes::Bytes;
use executor::channel;
use executor::channel::oneshot;

pub struct DataSideChannel {
    sender: channel::Sender<Packet>,
    receiver: channel::Receiver<Packet>
}

struct Packet {
    stream_id: u32,
    data: Bytes,
    returner: oneshot::Sender<()>,
}

impl DataSideChannel {
    pub fn create() -> Self {
        let (sender, receiver) = channel::unbounded();
        Self {
            sender,
            receiver
        }
    }

    /// NOTE: We intentionally block for the other end to receive the data before returning
    /// so that camera data is processed in a round robin manner across cameras with reasonable
    /// flow control.
    pub async fn push(&self, stream_id: u32, data: Bytes) -> Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.sender.send(Packet {
            stream_id,
            data,
            returner: sender
        }).await?;

        let _ = receiver.recv().await;

        Ok(())
    }

    pub async fn recv(&self) -> Result<(u32, Bytes)> {
        let packet = self.receiver.recv().await?;
        packet.returner.send(());
        Ok((packet.stream_id, packet.data))
    }

}