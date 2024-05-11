use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cluster_client::meta::client::ClusterMetaClient;
use common::errors::*;
use common::list::Appendable;
use crypto::random::SharedRng;
use executor::sync::AsyncMutex;
use executor::{channel, lock};
use nordic_tools_proto::nordic::*;
use nordic_wire::constants::{RadioAddress, LINK_IV_SIZE, LINK_KEY_SIZE};
use nordic_wire::packet::PacketBuffer;

use crate::link_util::generate_radio_address;
use crate::usb_radio::USBRadio;

const POLLING_INTERVAL: Duration = Duration::from_millis(100);

const PACKET_COUNTER_SAVE_INTERVAL: u32 = 1000;

pub struct RadioBridge {
    inner: RadioBridgeInner,
    radio: USBRadio,
    radio_event_receiver: channel::Receiver<()>,
}

#[derive(Clone)]
struct RadioBridgeInner {
    shared: Arc<Shared>,
}

struct Shared {
    state: AsyncMutex<State>,

    meta_client: ClusterMetaClient,

    /// Events an event to the radio thread whenever a new config/queue change
    /// occurs.
    radio_event_sender: channel::Sender<()>,
}

struct State {
    state_object_name: String,

    state_data: RadioBridgeStateData,

    // /// Packet counter of the last
    // last_packet_counter: u32,
    /// Packets which are pending being send to a remote device.
    send_queue: Vec<RadioBridgePacket>,

    /// Whether or not the NetworkConfig has changed since the last time it was
    /// pushed to the USB device.
    config_changed: bool,

    receivers: HashMap<RadioAddress, channel::Sender<RadioBridgePacket>>,
}

impl RadioBridge {
    pub async fn create(state_object_name: &str, usb: &usb::DeviceSelector) -> Result<Self> {
        let mut radio = USBRadio::find(&usb).await?;

        let mut meta_client = ClusterMetaClient::create_from_environment().await?;

        // TODO: We should ideally grab a lock on this key to ensure there aren't
        // concurrent mutations. We can cache this in memory so long as we monitor the
        // lock for failures and ensure that future writes check if changes since last
        // index.
        let state_data = match meta_client
            .get_object::<RadioBridgeStateData>(state_object_name)
            .await?
        {
            Some(v) => v,
            None => {
                println!("Creating new bridge config");

                let local_address = generate_radio_address().await?;

                let mut state_data = RadioBridgeStateData::default();
                state_data
                    .network_mut()
                    .address_mut()
                    .extend_from_slice(&local_address);

                meta_client
                    .set_object(state_object_name, &state_data)
                    .await?;

                state_data
            }
        };

        println!("Local address: {:02x?}", state_data.network().address());

        let (radio_event_sender, radio_event_receiver) = channel::bounded(1);

        Ok(Self {
            radio,
            radio_event_receiver,
            inner: RadioBridgeInner {
                shared: Arc::new(Shared {
                    meta_client,
                    state: AsyncMutex::new(State {
                        state_object_name: state_object_name.to_string(),
                        state_data: state_data.clone(),
                        // last_packet_counter: state_data.network().last_packet_counter(),
                        receivers: HashMap::new(),
                        send_queue: vec![],
                        config_changed: true,
                    }),
                    radio_event_sender,
                }),
            },
        })
    }

    pub fn add_services(&self, rpc_server: &mut rpc::Http2Server) -> Result<()> {
        rpc_server.add_service(self.inner.clone().into_service())?;
        Ok(())
    }

    pub async fn run(mut self) -> Result<()> {
        self.inner
            .radio_thread(self.radio, self.radio_event_receiver)
            .await
    }
}

impl RadioBridgeInner {
    async fn radio_thread(
        self,
        mut radio: USBRadio,
        event_receiver: channel::Receiver<()>,
    ) -> Result<()> {
        loop {
            let mut state = self.shared.state.lock().await?.enter();

            if state.config_changed {
                radio.set_network_config(state.state_data.network()).await?;
                state.config_changed = false;
            }

            while let Some(packet) = radio.recv_packet().await? {
                // TODO: Update the last received packet counter (and verify that we haven't
                // received an old packet).

                if let Some(receiver) = state.receivers.get(packet.remote_address()) {
                    // TODO: Add the name to these?
                    let mut packet_proto = RadioBridgePacket::default();
                    packet_proto.set_data(packet.data());

                    receiver.send(packet_proto).await?;
                }
            }

            while let Some(packet) = state.send_queue.pop() {
                let address = match self.name_to_address(packet.device_name(), &state) {
                    Some(addr) => addr,
                    // NOTE: If a device is removed shortly after a Send RPC, it may not be sent or
                    // return an error to the caller in this case.
                    None => continue,
                };

                let mut packet_buffer = PacketBuffer::new();
                // packet_buffer.set_counter(self.next_packet_counter(&mut state).await?);
                packet_buffer.remote_address_mut().copy_from_slice(&address);
                packet_buffer.resize_data(packet.data().len());
                packet_buffer.data_mut().copy_from_slice(packet.data());

                radio.send_packet(&packet_buffer).await?;
            }

            state.exit();

            let _ = executor::timeout(POLLING_INTERVAL, event_receiver.recv()).await;
        }
    }

    fn name_to_address(&self, name: &str, state: &State) -> Option<RadioAddress> {
        state
            .state_data
            .devices()
            .iter()
            .find(|device| device.name() == name)
            .map(|device| *array_ref![device.address(), 0, 4])
    }

    // TODO: Implement support for shifting all operations including packet counting
    // and encryption to the host.
    /*
    async fn next_packet_counter(&self, state: &mut State) -> Result<u32> {
        if state.last_packet_counter >= state.state_data.network().last_packet_counter() {
            let mut next_data = state.state_data.clone();
            next_data
                .network_mut()
                .set_last_packet_counter(state.last_packet_counter + PACKET_COUNTER_SAVE_INTERVAL);

            self.shared
                .meta_client
                .set_object(&state.state_object_name, &next_data)
                .await?;
            state.state_data = next_data;
        }

        state.last_packet_counter += 1;
        Ok(state.last_packet_counter)
    }
    */
}

#[async_trait]
impl RadioBridgeService for RadioBridgeInner {
    async fn ListDevices(
        &self,
        request: rpc::ServerRequest<protobuf_builtins::google::protobuf::Empty>,
        response: &mut rpc::ServerResponse<RadioBridgeListDevicesResponse>,
    ) -> Result<()> {
        let state = self.shared.state.lock().await?.read_exclusive();
        for dev in state.state_data.devices() {
            response.value.add_devices(dev.as_ref().clone());
        }

        response.set_bridge_address(state.state_data.network().address());

        Ok(())
    }

    async fn NewDevice(
        &self,
        request: rpc::ServerRequest<RadioBridgeNewDeviceRequest>,
        response: &mut rpc::ServerResponse<RadioBridgeNewDeviceResponse>,
    ) -> Result<()> {
        let address = generate_radio_address().await?;

        let mut link_key = vec![0u8; LINK_KEY_SIZE];
        let rng = crypto::random::global_rng();
        rng.generate_bytes(&mut link_key).await;

        let mut link_iv = vec![0u8; LINK_IV_SIZE];
        rng.generate_bytes(&mut link_iv).await;

        let mut state = self.shared.state.lock().await?.read_exclusive();

        if self
            .name_to_address(request.device().name(), &state)
            .is_some()
        {
            return Err(rpc::Status::already_exists("Device already exists").into());
        }

        // TODO: Also verify that we don't already have another device with the same
        // name.

        let mut next_data = state.state_data.clone();

        // TODO: Also support any other metadata provided in the request.
        let mut dev = RadioBridgeDevice::default();
        dev.address_mut().extend_from_slice(&address);
        dev.set_name(request.device().name());
        next_data.add_devices(dev.clone());

        let mut link = nordic_proto::nordic::Link::default();
        link.set_address(&address[..]);
        link.set_key(&link_key[..]);
        link.set_iv(&link_iv[..]);
        next_data.network_mut().add_links(link);

        self.shared
            .meta_client
            .set_object(&state.state_object_name, &next_data)
            .await?;

        // Populate the response
        response.set_device(dev);

        response.network_config_mut().set_address(&address[..]);

        let mut dev_link = nordic_proto::nordic::Link::default();
        dev_link.set_address(state.state_data.network().address());
        dev_link.set_key(&link_key[..]);
        dev_link.set_iv(&link_iv[..]);

        response.network_config_mut().add_links(dev_link);

        lock!(state <= state.upgrade(), {
            state.state_data = next_data;
            state.config_changed = true;
        });

        let _ = self.shared.radio_event_sender.try_send(());

        Ok(())
    }

    async fn RemoveDevice(
        &self,
        request: rpc::ServerRequest<RadioBridgeRemoveDeviceRequest>,
        response: &mut rpc::ServerResponse<protobuf_builtins::google::protobuf::Empty>,
    ) -> Result<()> {
        let mut state = self.shared.state.lock().await?.read_exclusive();

        let mut next_data = state.state_data.clone();

        let mut device = None;
        {
            let devs: &mut Vec<protobuf::MessagePtr<RadioBridgeDevice>> = next_data.devices_mut();
            for i in 0..devs.len() {
                if devs[i].name() == request.device_name() {
                    device = Some(devs.remove(i));
                    break;
                }
            }
        }

        let device =
            device.ok_or_else(|| Error::from(rpc::Status::not_found("No device found")))?;

        for i in 0..next_data.network().links_len() {
            if next_data.network().links()[i].address() == device.address() {
                next_data.network_mut().links_mut().remove(i);
                break;
            }
        }

        if state.receivers.contains_key(device.address()) {
            return Err(rpc::Status::failed_precondition(
                "Can't remove a device which has an active subscriber",
            )
            .into());
        }

        self.shared
            .meta_client
            .set_object(&state.state_object_name, &state.state_data)
            .await?;

        lock!(state <= state.upgrade(), {
            state.state_data = next_data;
            state.config_changed = true;
        });

        let _ = self.shared.radio_event_sender.try_send(());

        Ok(())
    }

    async fn Send(
        &self,
        request: rpc::ServerRequest<RadioBridgePacket>,
        response: &mut rpc::ServerResponse<protobuf_builtins::google::protobuf::Empty>,
    ) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if self
                .name_to_address(request.device_name(), &state)
                .is_none()
            {
                return Err(rpc::Status::not_found("No such device"));
            }

            state.send_queue.push(request.value);
            Ok(())
        })?;

        let _ = self.shared.radio_event_sender.try_send(());

        Ok(())
    }

    async fn Receive(
        &self,
        request: rpc::ServerRequest<RadioReceiveRequest>,
        response: &mut rpc::ServerStreamResponse<RadioBridgePacket>,
    ) -> Result<()> {
        let reg = lock!(state <= self.shared.state.lock().await?, {
            // Resolve the device name to an address.
            let address = self
                .name_to_address(request.device_name(), &state)
                .ok_or_else(|| {
                    rpc::Status::not_found(format!(
                        "No registered device named: {}",
                        request.device_name()
                    ))
                })?;

            let (sender, receiver) = channel::unbounded();
            if state.receivers.contains_key(&address) {
                return Err(rpc::Status::aborted(
                    "Device already has another receiver registered",
                ));
            }

            state.receivers.insert(address, sender);
            Ok(ReceiverRegistration {
                address,
                receiver,
                bridge: self,
            })
        })?;

        loop {
            match reg.receiver.recv().await {
                Ok(v) => {
                    response.send(v).await?;
                }
                Err(_) => {
                    return Err(
                        rpc::Status::aborted("Device was reconfigured while listening").into(),
                    );
                }
            }
        }
    }
}

struct ReceiverRegistration<'a> {
    address: RadioAddress,
    receiver: channel::Receiver<RadioBridgePacket>,
    bridge: &'a RadioBridgeInner,
}

impl<'a> Drop for ReceiverRegistration<'a> {
    fn drop(&mut self) {
        let inst = self.bridge.clone();
        let address = self.address.clone();
        executor::spawn(async move {
            lock!(state <= inst.shared.state.lock().await.unwrap(), {
                state.receivers.remove(&address);
            });
        });
    }
}
