use core::sync::atomic::AtomicUsize;

use common::const_default::ConstDefault;
use common::segmented_buffer::SegmentedBuffer;
use crypto::ccm::CCM;
use executor::channel::Channel;
use executor::futures::*;
use executor::mutex::{Mutex, MutexGuard};
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::NetworkConfig;

use crate::ecb::*;
use crate::log;
use crate::radio::Radio;

/// Size to use for all buffers. This is also the maximum size that we will
/// transmit or receive in one transaction.
const BUFFER_SIZE: usize = 256;

// /// Messages are sent on this channel from the USB thread to the Radio thread
// /// when there is data present in the transmit buffer to be sent.
// static TRANSMIT_PENDING: Channel<()> = Channel::new();

// struct TransmitBuffer {
//     last_packet_counter: u32,
//     buffer: SegmentedBuffer<[u8; BUFFER_SIZE]>,
// }

// impl TransmitBuffer {
//     pub const fn new() -> Self {
//         Self {
//             /// Value of the last packet counter which we've sent.
//             last_packet_counter: 0,
//             buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
//         }
//     }
// }

const CCM_LENGTH_SIZE: usize = 2;
const CCM_NONCE_SIZE: usize = 13; // 15 - CCM_LENGTH_SIZE
const CCM_TAG_SIZE: usize = 4;

pub struct RadioSocket {
    transmit_pending: Channel<()>,
    state: Mutex<RadioSocketState>,
}

struct RadioSocketState {
    network: NetworkConfig,

    /// Whether or not 'network' contains a valid config for sending/receiving
    /// packets.
    network_valid: bool,

    transmit_buffer: SegmentedBuffer<[u8; BUFFER_SIZE]>,

    receive_buffer: SegmentedBuffer<[u8; BUFFER_SIZE]>,
}

impl RadioSocket {
    pub const fn new() -> Self {
        Self {
            transmit_pending: Channel::new(),
            state: Mutex::new(RadioSocketState {
                network: NetworkConfig::DEFAULT,
                network_valid: false,
                transmit_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
                receive_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
            }),
        }
    }

    pub async fn lock_network_config<'a>(&'a self) -> RadioNetworkConfigGuard<'a> {
        let state_guard = self.state.lock().await;
        RadioNetworkConfigGuard { state_guard }
    }

    pub async fn set_network_config(&self, config: NetworkConfig) {
        let mut state = self.state.lock().await;
        state.network = config;
        // TODO: Validate the config.
        state.network_valid = true;
        drop(state);

        // Notify the RadioController that a change has occured in case it is waiting
        // for one.
        self.transmit_pending.try_send(()).await;
    }

    /// Enqueues data to be transmitted over the radio via a global queue.
    ///
    /// NOTE: This returns as soon as the packet is queued. The packet may need
    /// be sent if there is a failure or it is overwritten by future enqueueing
    /// that overflows the internal buffer.
    ///
    /// - packet.address() must be set to the address of the remote device to
    ///   which we should send this packet.
    /// - After returning packet.counter() will be set by this function to the
    ///   value of the counter that will be used to send this packet.
    pub async fn enqueue_tx(&self, packet: &mut PacketBuffer) {
        // In the packet, the from address will be used as a 'to address'

        let mut state = self.state.lock().await;

        let counter = state.network.last_packet_counter() + 1;
        state.network.set_last_packet_counter(counter);

        // TODO: If enough counter increments have occured, save to EEPROM.

        packet.set_counter(counter);

        packet.write_to(&mut state.transmit_buffer);
        drop(state);

        // TODO: Make sure that this doesn't block if the channel is full.
        self.transmit_pending.try_send(()).await;
    }

    /// Retrieves the next already received remote packet.
    ///
    /// If a packet has already been received, true is returned and the given
    /// packet object is filled with its data. Otherwise, this function will
    /// immediately return false (does not block for a packet to be received).
    #[must_use]
    pub async fn dequeue_rx(&self, packet: &mut PacketBuffer) -> bool {
        let mut state = self.state.lock().await;
        packet.read_from(&mut state.receive_buffer)
    }

    /// NOTE: This should only be called by the RadioController when there are
    /// no other pending waits on the transmit_pending channel.
    async fn get_valid_state<'a>(&'a self) -> MutexGuard<'a, RadioSocketState> {
        loop {
            let state = self.state.lock().await;
            if state.network_valid {
                return state;
            }

            drop(state);
            self.transmit_pending.recv().await;
        }
    }
}

pub struct RadioNetworkConfigGuard<'a> {
    state_guard: MutexGuard<'a, RadioSocketState>,
}

impl<'a> RadioNetworkConfigGuard<'a> {
    pub fn get(&self) -> &NetworkConfig {
        &self.state_guard.network
    }

    // NOTE: This doesn't provide any mutation handlers as mutating the config
    // requires sending out a notification.
}

pub struct RadioController {
    socket: &'static RadioSocket,
    radio: Radio,
    ecb: ECB,
}

impl RadioController {
    pub fn new(socket: &'static RadioSocket, radio: Radio, ecb: ECB) -> Self {
        Self { socket, radio, ecb }
    }

    pub async fn run(mut self) {
        enum Event {
            Received,
            TransmitPending,
        }

        // TODO: Move this into the instance?
        let mut packet_buf = PacketBuffer::new();

        loop {
            // Step 1: Lock the state and verify that it is ok.

            let socket_state = self.socket.get_valid_state().await;

            // Prepare for receiving packets addressed to us.
            self.radio
                .set_address(array_ref![socket_state.network.address(), 0, 4]);

            drop(socket_state);

            // TODO: Implement a more efficient way to cancel the receive future.
            let event = race2(
                map(self.radio.receive_packet(packet_buf.raw_mut()), |_| {
                    Event::Received
                }),
                map(self.socket.transmit_pending.recv(), |_| {
                    Event::TransmitPending
                }),
            )
            .await;

            let mut socket_state = self.socket.get_valid_state().await;

            match event {
                Event::Received => {
                    log!(b"RADIO RX ");

                    // TODO: We need to check for the case that the radio packet gets truncated (the
                    // first length byte indicates a length that is larger than the buffer size).

                    // for i in 0..packet_buf.as_bytes().len() {
                    //     log!(crate::num_to_slice(packet_buf.raw()[i] as u32).as_ref());
                    //     log!(b", ");
                    // }

                    // log!(&temp_buffer[0..n]);
                    log!(b"\n");

                    let from_address = packet_buf.remote_address();

                    let (valid_keys, key, nonce) = self.generate_ccm_key_pair(
                        &socket_state.network,
                        &packet_buf,
                        from_address,
                        from_address,
                    );

                    if !valid_keys {
                        log!(b"Unknown peer\n");
                        continue;
                    }

                    let mut ccm = CCM::new(
                        AES128BlockBuffer::new(&key, &mut self.ecb),
                        CCM_TAG_SIZE,
                        CCM_LENGTH_SIZE,
                        &nonce,
                    );

                    if let Err(_) = ccm.decrypt_inplace(packet_buf.ciphertext_mut(), &[]) {
                        log!(b"EFAIL\n");
                        continue;
                    }

                    packet_buf.write_to(&mut socket_state.receive_buffer);

                    drop(socket_state);
                }
                Event::TransmitPending => {
                    // Assuming we have a packet buffer,

                    log!(b"RADIO TX\n");

                    // NOTE: The packet will contain the TO_ADDRESS.
                    let got_packet = packet_buf.read_from(&mut socket_state.transmit_buffer);

                    if !got_packet {
                        continue;
                    }

                    let from_address = array_ref![socket_state.network.address(), 0, 4];
                    let to_address = *packet_buf.remote_address();

                    // Use our local address in the packet so that from the receiving device's
                    // perspective, the remote_address is correct.
                    packet_buf
                        .remote_address_mut()
                        .copy_from_slice(from_address);

                    let (valid_keys, key, nonce) = self.generate_ccm_key_pair(
                        &socket_state.network,
                        &packet_buf,
                        &to_address,
                        from_address,
                    );

                    if !valid_keys {
                        log!(b"Unknown peer\n");
                        continue;
                    }

                    // .await;

                    drop(socket_state);

                    let mut ccm = CCM::new(
                        AES128BlockBuffer::new(&key, &mut self.ecb),
                        CCM_TAG_SIZE,
                        CCM_LENGTH_SIZE,
                        &nonce,
                    );
                    ccm.encrypt_inplace(packet_buf.ciphertext_mut(), &[]);

                    self.radio.set_address(&to_address);

                    log!(b"T ");
                    log!(crate::num_to_slice(packet_buf.as_bytes().len() as u32).as_ref());
                    log!(b"\n");

                    self.radio.send_packet(packet_buf.as_bytes()).await;
                }
            }
        }
    }

    fn generate_ccm_key_pair(
        &self,
        network: &NetworkConfig,
        packet_buf: &PacketBuffer,
        remote_address: &[u8; 4],
        from_address: &[u8; 4],
    ) -> (bool, [u8; 16], [u8; CCM_NONCE_SIZE]) {
        // TODO: Remove the unwrap.
        let link = match network
            .links()
            .iter()
            .find(|l| l.address() == &remote_address[..])
        {
            Some(l) => l,
            None => {
                // TODO: Return an error.
                return (false, [0u8; 16], [0u8; CCM_NONCE_SIZE]);
            }
        };

        let mut nonce = [0u8; CCM_NONCE_SIZE];
        nonce[0..4].copy_from_slice(&packet_buf.counter().to_le_bytes());
        // TODO: Check the sizes of these?
        nonce[4..8].copy_from_slice(from_address);
        nonce[8..].copy_from_slice(link.iv());

        (true, *array_ref![link.key(), 0, 16], nonce)
    }
}
