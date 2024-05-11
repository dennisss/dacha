use common::list::Appendable;
use executor::channel::Channel;
use executor::lock;
use executor::sync::AsyncMutex;

use common::fixed::vec::FixedVec;

use crate::usb::aligned::Aligned;
use crate::usb::controller::MAX_PACKET_SIZE;

/// Stores the next Interrupt/Bulk packet which should be transferred to the
/// host the next time the host requests a transfer.
pub struct USBDeviceSendBuffer {
    state: AsyncMutex<State>,
    channel: Channel<()>,
}

struct State {
    data: FixedVec<u8, MAX_PACKET_SIZE>,
}

impl USBDeviceSendBuffer {
    pub const fn new() -> Self {
        Self {
            state: AsyncMutex::new(State {
                data: FixedVec::new(),
            }),
            channel: Channel::new(),
        }
    }

    /// NOTE: If the buffer already contains any data it will be replaced.
    pub async fn write(&self, data: &[u8]) {
        lock!(state <= self.state.lock().await.unwrap(), {
            state.data.clear();
            state.data.extend_from_slice(data);
        });

        let _ = self.channel.try_send(()).await;
    }

    pub async fn try_read(&self) -> Option<FixedVec<u8, MAX_PACKET_SIZE>> {
        lock!(state <= self.state.lock().await.unwrap(), {
            if state.data.is_empty() {
                return None;
            }

            let ret = state.data.clone();
            state.data.clear();
            Some(ret)
        })
    }

    pub async fn wait_until_readable(&self) {
        loop {
            let done = lock!(state <= self.state.lock().await.unwrap(), {
                !state.data.is_empty()
            });

            if done {
                return;
            }

            let _ = self.channel.recv().await;
        }
    }
}
