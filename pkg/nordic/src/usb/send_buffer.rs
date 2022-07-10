use executor::channel::Channel;
use executor::mutex::Mutex;

use crate::usb::aligned::Aligned;
use crate::usb::controller::MAX_PACKET_SIZE;

pub struct USBDeviceSendBuffer {
    state: Mutex<State>,
}

struct State {
    data: Aligned<[u8; MAX_PACKET_SIZE], u32>,
    channel: Channel<()>,
    size: usize,
}
