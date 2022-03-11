use core::sync::atomic::AtomicUsize;

use common::const_default::ConstDefault;
use common::errors::*;
use common::segmented_buffer::SegmentedBuffer;
use crypto::ccm::CCM;
use executor::channel::Channel;
use executor::futures::*;
use executor::mutex::{Mutex, MutexGuard};
use nordic_proto::packet::PacketBuffer;
use nordic_proto::packet_cipher::PacketCipher;
use nordic_proto::proto::net::NetworkConfig;

use crate::config_storage::NetworkConfigStorage;
use crate::ecb::*;
use crate::log;
use crate::radio::Radio;

/// Size to use for all buffers. This is also the maximum size that we will
/// transmit or receive in one transaction.
const BUFFER_SIZE: usize = 256;

const CCM_LENGTH_SIZE: usize = 2;
const CCM_NONCE_SIZE: usize = 13; // 15 - CCM_LENGTH_SIZE
const CCM_TAG_SIZE: usize = 4;

const PACKET_COUNTER_SAVE_INTERVAL: u32 = 100;

#[derive(Clone, Copy, Debug, Errable)]
#[repr(u32)]
pub enum RadioSocketError {
    SendingInvalidCounter,
}

pub struct RadioSocket {
    /// The presence of a value in this channel signals to the radio controller
    /// thread that there is data present in the state.transmit_buffer that
    /// should be sent.
    ///
    /// This is also re-used to wake up the radio thread when a network config
    /// change has occured.
    transmit_pending: Channel<()>,

    state: Mutex<RadioSocketState>,
}

struct RadioSocketState {
    network: NetworkConfig,

    /// Whether or not 'network' contains a valid config for sending/receiving
    /// packets.
    network_valid: bool,

    /// Last packet counter sent to the remote device.
    last_packet_counter: u32,

    network_storage: Option<NetworkConfigStorage>,

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
                last_packet_counter: 0,
                network_storage: None,
                transmit_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
                receive_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
            }),
        }
    }

    /// Configures a storage implementation for reading/writing NetworkConfigs
    /// durably.
    ///
    /// This will initialize the socket's config with any value present in the
    /// given storage and will write any network configs to it in the future.
    ///
    /// NOTE: This must be done before the socket is used.
    pub async fn configure_storage(&self, mut network_storage: NetworkConfigStorage) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        assert!(state.network_storage.is_none() && !state.network_valid);

        // TODO: Re-use the set_network_config code.
        if network_storage.read(&mut state.network).await? {
            // TODO: Check that all fields are set, etc.
            state.network_valid = true;
        }

        state.network_storage = Some(network_storage);

        Ok(())
    }

    pub async fn lock_network_config<'a>(&'a self) -> RadioNetworkConfigGuard<'a> {
        let state_guard = self.state.lock().await;
        RadioNetworkConfigGuard { state_guard }
    }

    pub async fn set_network_config(&self, config: NetworkConfig) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        state.network_valid = false;
        state.last_packet_counter = config.last_packet_counter();
        state.network = config;

        // Must clear these as these weren't generated using the latest config (so
        // things like packet counters may be stale).
        state.transmit_buffer.clear();
        state.receive_buffer.clear();

        if let Some(storage) = &mut state.network_storage {
            storage.write(&state.network).await?;
        }

        // NOTE: We only consider the network to be valid if it was successfully written
        // to storage.
        state.network_valid = true;

        drop(state);

        // Notify the RadioController that a change has occured in case it is waiting
        // for one.
        self.transmit_pending.try_send(()).await;

        Ok(())
    }

    /// Enqueues data to be transmitted over the radio via a global queue.
    ///
    /// NOTE: This returns as soon as the packet is queued. The packet may need
    /// be sent if there is a failure or it is overwritten by future enqueueing
    /// that overflows the internal buffer.
    ///
    /// - packet.address() must be set to the address of the remote device to
    ///   which we should send this packet.
    /// - packet.counter() should be set to 0 if this device manages its own
    ///   network storage. Otherwise it must be explicitly set to a non-zero
    ///   value.
    /// - After returning packet.counter() will be set by this function to the
    ///   value of the counter that will be used to send this packet.
    pub async fn enqueue_tx(&self, packet: &mut PacketBuffer) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        if !state.network_valid {
            return Err(RadioSocketError::SendingInvalidCounter.into());
        }

        if packet.counter() == 0 {
            // Generate a new counter.
            // Requires us to do our own persistence of it.

            let storage = state
                .network_storage
                .as_mut()
                .ok_or_else(|| Error::from(RadioSocketError::SendingInvalidCounter))?;

            // If we have hit the value in the config proto, then we must advance the config
            // proto and store it in EEPROM. NOTE: This must be atomic to avoid
            // using un-persisted counters if the write() fails.
            if state.last_packet_counter >= state.network.last_packet_counter() {
                // TODO: Implment this without the copy. e.g. instead we could have a
                // 'pending_persistance' flag or somethimg.
                let mut next_config = state.network.clone();
                next_config.set_last_packet_counter(
                    state.last_packet_counter + PACKET_COUNTER_SAVE_INTERVAL,
                );

                storage.write(&next_config).await?;

                state.network = next_config;
            }

            state.last_packet_counter += 1;
            packet.set_counter(state.last_packet_counter);
        } else {
            // We were provided an externally generated counter.

            if state.network_storage.is_some() {
                return Err(RadioSocketError::SendingInvalidCounter.into());
            }

            if state.last_packet_counter >= packet.counter() {
                return Err(RadioSocketError::SendingInvalidCounter.into());
            }

            state.last_packet_counter = packet.counter();
            state.network.set_last_packet_counter(packet.counter());
        }

        let counter = state.network.last_packet_counter() + 1;
        state.network.set_last_packet_counter(counter);

        packet.write_to(&mut state.transmit_buffer);
        drop(state);

        // TODO: Make sure that this doesn't block if the channel is full.
        self.transmit_pending.try_send(()).await;

        Ok(())
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
                    log!(b"RADIO RX\n");

                    // TODO: We need to check for the case that the radio packet gets truncated (the
                    // first length byte indicates a length that is larger than the buffer size).

                    // for i in 0..packet_buf.as_bytes().len() {
                    //     log!(crate::log::num_to_slice(packet_buf.raw()[i] as u32).as_ref());
                    //     log!(b", ");
                    // }

                    let from_address = *packet_buf.remote_address();

                    let ecb = &mut self.ecb;
                    let packet_encryptor = match PacketCipher::create(
                        &mut packet_buf,
                        &socket_state.network,
                        |key| AES128BlockBuffer::new(key, &mut self.ecb),
                        &from_address,
                        &from_address,
                    ) {
                        Ok(v) => v,
                        Err(_) => {
                            continue;
                        }
                    };

                    // TODO: Considering dropping the socket_state lock here if decrypt() is ever
                    // implemented with async.

                    if let Err(_) = packet_encryptor.decrypt() {
                        log!(b"EFAIL\n");
                        continue;
                    }

                    // TODO: Record the newly received packet counter.

                    socket_state.receive_buffer.write(packet_buf.as_bytes());

                    drop(socket_state);
                }
                Event::TransmitPending => {
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

                    let packet_encryptor = match PacketCipher::create(
                        &mut packet_buf,
                        &socket_state.network,
                        |key| AES128BlockBuffer::new(key, &mut self.ecb),
                        &to_address,
                        from_address,
                    ) {
                        Ok(v) => v,
                        Err(_) => {
                            continue;
                        }
                    };

                    drop(socket_state);

                    if let Err(_) = packet_encryptor.encrypt() {
                        continue;
                    }

                    self.radio.set_address(&to_address);
                    self.radio.send_packet(packet_buf.as_bytes()).await;
                }
            }
        }
    }
}
