use core::sync::atomic::AtomicUsize;

use common::const_default::ConstDefault;
use common::errors::*;
use common::list::Appendable;
use common::segmented_buffer::SegmentedBuffer;
use crypto::ccm::CCM;
use executor::channel::Channel;
use executor::futures::*;
use executor::sync::{Mutex, MutexGuard};
use nordic_proto::packet::PacketBuffer;
use nordic_proto::packet_cipher::PacketCipher;
use nordic_proto::proto::net::{LinkState, NetworkConfig, NetworkState};

use crate::ecb::*;
use crate::params::{ParamsStorage, NETWORK_CONFIG_ID, NETWORK_STATE_ID};
use crate::radio::Radio;

/// Size to use for all buffers. This is also the maximum size that we will
/// transmit or receive in one transaction.
const BUFFER_SIZE: usize = 256;

const CCM_LENGTH_SIZE: usize = 2;
const CCM_NONCE_SIZE: usize = 13; // 15 - CCM_LENGTH_SIZE
const CCM_TAG_SIZE: usize = 4;

const PACKET_COUNTER_SAVE_INTERVAL: u32 = 1000;

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

    receive_pending: Channel<()>,

    state: Mutex<RadioSocketState>,
}

struct RadioSocketState {
    network_config: NetworkConfig,

    network_state: NetworkState,

    /// Whether or not 'network_config' contains a valid config for
    /// sending/receiving packets.
    network_valid: bool,

    /// Last value of network_state.last_packet_counter() persisted to local
    /// durable storage.
    persisted_packet_counter: u32,

    params_storage: Option<&'static ParamsStorage>,

    transmit_buffer: SegmentedBuffer<[u8; BUFFER_SIZE]>,

    receive_buffer: SegmentedBuffer<[u8; BUFFER_SIZE]>,
}

impl RadioSocket {
    pub const fn new() -> Self {
        Self {
            transmit_pending: Channel::new(),
            receive_pending: Channel::new(),
            state: Mutex::new(RadioSocketState {
                network_config: NetworkConfig::DEFAULT,
                network_state: NetworkState::DEFAULT,
                network_valid: false,
                persisted_packet_counter: 0,
                params_storage: None,
                transmit_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
                receive_buffer: SegmentedBuffer::new([0u8; BUFFER_SIZE]),
            }),
        }
    }

    /// Configures a storage implementation for reading/writing
    /// NetworkConfigs/NetworkStates durably.
    ///
    /// This will initialize the socket's config with any value present in the
    /// given storage and will write any network parameters to it in the future.
    ///
    /// NOTE: This must be done before the socket is used.
    pub async fn configure_storage(
        &self,
        mut params_storage: &'static ParamsStorage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        assert_no_debug!(state.params_storage.is_none() && !state.network_valid);

        // TODO: Re-use the set_network_config code.

        let found_config = params_storage
            .read_into_proto(NETWORK_CONFIG_ID, &mut state.network_config)
            .await?;

        let found_state = params_storage
            .read_into_proto(NETWORK_STATE_ID, &mut state.network_state)
            .await?;

        state.network_valid = found_config && Self::is_valid_config(&state.network_config);

        if state.network_valid && !found_state {
            state.network_state = NetworkState::DEFAULT;
        }

        state.persisted_packet_counter = state.network_state.last_packet_counter();

        state.params_storage = Some(params_storage);

        if state.network_valid {
            log!("Read valid config from storage.");
        } else {
            log!("No valid config available in storage.");
        }

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
        state.network_config = config;

        if let Some(storage) = &mut state.params_storage {
            storage
                .write_proto(NETWORK_CONFIG_ID, &state.network_config)
                .await?;
        }

        let is_valid = Self::is_valid_config(&state.network_config);
        if is_valid {
            log!("Set valid");
        } else {
            log!("Set INVALID");
        }

        // NOTE: We only consider the network to be valid if it was successfully written
        // to storage.
        state.network_valid = is_valid;

        if is_valid {
            Self::cleanup_network_state(state);
        }

        drop(state);

        // Notify the RadioController that a change has occured in case it is waiting
        // for one.
        self.transmit_pending.try_send(()).await;

        Ok(())
    }

    // Also must validate the state
    fn is_valid_config(config: &NetworkConfig) -> bool {
        if config.address().len() != nordic_proto::constants::RADIO_ADDRESS_SIZE {
            return false;
        }

        for link in config.links() {
            if link.address().len() != nordic_proto::constants::RADIO_ADDRESS_SIZE
                || link.iv().len() != nordic_proto::constants::LINK_IV_SIZE
                || link.key().len() != nordic_proto::constants::LINK_KEY_SIZE
            {
                return false;
            }
        }

        true
    }

    fn cleanup_network_state(state: &mut RadioSocketState) {
        // All links not configured should be removed from the network_state.
        let mut i = 0;
        while i < state.network_state.links().len() {
            let mut found = false;
            for config_link in state.network_config.links() {
                if config_link.address() == state.network_state.links()[i].address() {
                    found = true;
                    break;
                }
            }

            if !found {
                state.network_state.links_mut().swap_remove(i);
                continue;
            }

            i += 1;
        }
    }

    /// Enqueues data to be transmitted over the radio via a global queue.
    ///
    /// NOTE: This returns as soon as the packet is queued. The packet may need
    /// be sent if there is a failure or it is overwritten by future enqueueing
    /// that overflows the internal buffer.
    ///
    /// - packet.address() must be set to the address of the remote device to
    ///   which we should send this packet.
    /// - packet.counter() should be set to 0.
    /// - After this function returns, packet.counter() will be set to the value
    ///   of the counter that will be used to send this packet.
    pub async fn enqueue_tx(&self, packet: &mut PacketBuffer) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        let state = &mut *state_guard;

        if !state.network_valid || packet.counter() != 0 {
            return Err(RadioSocketError::SendingInvalidCounter.into());
        }

        let storage = state
            .params_storage
            .as_mut()
            .ok_or_else(|| Error::from(RadioSocketError::SendingInvalidCounter))?;

        // Ensure that we have persisted at least the current packet counter.
        // NOTE: This must be atomic in case the write fails.
        // TODO: Validate this works correctly.
        if state.persisted_packet_counter <= state.network_state.last_packet_counter() {
            let mut new_state = state.network_state.clone();
            new_state.set_last_packet_counter(
                new_state.last_packet_counter() + PACKET_COUNTER_SAVE_INTERVAL,
            );
            storage.write_proto(NETWORK_STATE_ID, &new_state).await?;
            state.persisted_packet_counter = new_state.last_packet_counter();
        }

        // Generate new packet counter incrementally.
        let packet_counter = state.network_state.last_packet_counter() + 1;
        state.network_state.set_last_packet_counter(packet_counter);
        packet.set_counter(packet_counter);

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
        state.receive_buffer.read(packet.raw_mut()).is_some()
    }

    /// Waits until a packet can be read with dequeue_rx.
    pub async fn wait_for_rx(&self) {
        loop {
            {
                let mut state = self.state.lock().await;
                if !state.receive_buffer.is_empty() {
                    return;
                }
            }

            self.receive_pending.recv().await;
        }
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
    pub fn get(&self) -> Option<&NetworkConfig> {
        if !self.state_guard.network_valid {
            return None;
        }

        Some(&self.state_guard.network_config)
    }

    // NOTE: This doesn't provide any mutation handlers as mutating the config
    // requires sending out a notification.
}

define_thread!(
    RadioControllerThread,
    radio_controller_thread_fn,
    radio_controller: RadioController
);
async fn radio_controller_thread_fn(radio_controller: RadioController) {
    radio_controller.run().await;
}

pub struct RadioController {
    socket: &'static RadioSocket,
    radio: Radio,
    ecb: ECB,

    tx_event: Option<&'static Channel<()>>,
    rx_event: Option<&'static Channel<()>>,
}

impl RadioController {
    pub fn new(socket: &'static RadioSocket, radio: Radio, ecb: ECB) -> Self {
        Self {
            socket,
            radio,
            ecb,
            tx_event: None,
            rx_event: None,
        }
    }

    pub fn set_tx_event(&mut self, event: &'static Channel<()>) {
        self.tx_event = Some(event);
    }

    pub fn set_rx_event(&mut self, event: &'static Channel<()>) {
        self.rx_event = Some(event);
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
                .set_address(array_ref![socket_state.network_config.address(), 0, 4]);

            drop(socket_state);

            // TODO: Implement a more efficient way to cancel the receive future.
            let event = race!(
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
                    log!("RADIO RX");

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
                        &socket_state.network_config,
                        |key| AES128BlockBuffer::new(key, &mut self.ecb),
                        &from_address,
                        &from_address,
                    ) {
                        Ok(v) => v,
                        Err(_) => {
                            log!("EFAIL1");
                            continue;
                        }
                    };

                    // TODO: Considering dropping the socket_state lock here if decrypt() is ever
                    // implemented with async.

                    if let Err(_) = packet_encryptor.decrypt() {
                        log!("EFAIL2");
                        continue;
                    }

                    // Find the link state associated with the remove device.
                    //
                    // NOTE: We do this after validating that the packet is from a known sender to
                    // avoid being able to polute the links vector.
                    let mut link_state = {
                        match socket_state
                            .network_state
                            .links_mut()
                            .iter_mut()
                            .find(|v| v.address() == &from_address)
                        {
                            Some(v) => v,
                            None => {
                                // Create a new entry.
                                let mut new_state = LinkState::default();
                                new_state.address_mut().extend_from_slice(&from_address);
                                socket_state.network_state.links_mut().push(new_state);
                                socket_state.network_state.links_mut().last_mut().unwrap()
                            }
                        }
                    };

                    // Block receiving old packets.
                    if link_state.last_packet_counter() >= packet_buf.counter() {
                        log!("RX: Old packet");
                        continue;
                    }

                    link_state.set_last_packet_counter(packet_buf.counter());

                    if let Some(e) = &self.rx_event {
                        e.try_send(()).await;
                    }

                    // TODO: Record the newly received packet counter.

                    socket_state.receive_buffer.write(packet_buf.as_bytes());
                    let _ = self.socket.receive_pending.try_send(()).await;

                    drop(socket_state);
                }
                Event::TransmitPending => {
                    log!("RADIO TX");

                    // NOTE: The packet will contain the TO_ADDRESS.
                    let got_packet = packet_buf.read_from(&mut socket_state.transmit_buffer);

                    if !got_packet {
                        continue;
                    }

                    let from_address = array_ref![socket_state.network_config.address(), 0, 4];
                    let to_address = *packet_buf.remote_address();

                    // Use our local address in the packet so that from the receiving device's
                    // perspective, the remote_address is correct.
                    packet_buf
                        .remote_address_mut()
                        .copy_from_slice(from_address);

                    let packet_encryptor = match PacketCipher::create(
                        &mut packet_buf,
                        &socket_state.network_config,
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

                    if let Some(e) = &self.tx_event {
                        e.try_send(()).await;
                    }

                    self.radio.set_address(&to_address);
                    self.radio.send_packet(packet_buf.as_bytes()).await;
                }
            }
        }
    }
}
