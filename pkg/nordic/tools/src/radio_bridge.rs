use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cluster_client::ClusterMetaClient;
use common::errors::*;
use common::list::Appendable;
use crypto::random::SharedRng;
use executor::sync::AsyncMutex;
use executor_multitask::*;
use executor::{channel, lock};
use nordic_tools_proto::nordic::*;
use nordic_wire::constants::{RadioAddress, LINK_IV_SIZE, LINK_KEY_SIZE};
use nordic_wire::packet::PacketBuffer;
use nordic_driver::usb_radio::USBRadio;
use peripherals_service::device::PeripheralsDevice;

use crate::link_util::generate_radio_address;

const POLLING_INTERVAL: Duration = Duration::from_millis(100);

const PACKET_COUNTER_SAVE_INTERVAL: u32 = 1000;

pub struct RadioBridge {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(RadioBridge, resources);

struct Shared {
    state: AsyncMutex<State>,

    device: Arc<PeripheralsDevice>,

    meta_client: Arc<ClusterMetaClient>,

    /// Events an event to the radio thread whenever a new config/queue change
    /// occurs.
    radio_event_sender: channel::Sender<()>,
}

#[derive(Clone)]
struct RadioBridgeInner {
    shared: Arc<Shared>,
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
    pub async fn create(
        meta_client: Arc<ClusterMetaClient>,
        state_object_name: &str
    ) -> Result<Self> {
        let resources = ServiceResourceGroup::new("RadioBridge");

        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"radio_bridge_dongle")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        // TODO: Add as a Resource.
        let device = Arc::new(device);

        // TODO: We should ideally grab a lock on this key to ensure there aren't
        // concurrent mutations. We can cache this in memory so long as we monitor the
        // lock for failures and ensure that future writes check if changes since last
        // index.
        let state_data = match meta_client
            .get_object_proto::<RadioBridgeStateData>(state_object_name)
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
                    .set_object_proto(state_object_name, &state_data)
                    .await?;

                state_data
            }
        };

        println!("Local address: {:02x?}", state_data.network().address());

        let (radio_event_sender, radio_event_receiver) = channel::bounded(1);

        let shared = Arc::new(Shared {
            meta_client,
            device,
            state: AsyncMutex::new(State {
                state_object_name: state_object_name.to_string(),
                state_data: state_data.clone(),
                // last_packet_counter: state_data.network().last_packet_counter(),
                receivers: HashMap::new(),
                send_queue: vec![],
                config_changed: true,
            }),
            radio_event_sender,
        });

        // TODO: NEed a periodic request to verify MCU liveness.

        resources.spawn_interruptable("TX", Self::tx_thread(shared.clone(), radio_event_receiver)).await;
        resources.spawn_interruptable("RX", Self::rx_thread(shared.clone())).await;
        // resources.spawn_interruptable("Log", Self::log_thread(shared.clone())).await;

        Ok(Self {
            resources,
            shared
        })
    }

    async fn log_thread(shared: Arc<Shared>) -> Result<()> {
        loop {

            let log = shared.device.raw().read_log_entries().await?;
            if !log.is_empty() {
                eprintln!("Log: {:?}", log);
            }

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    async fn rx_thread(shared: Arc<Shared>) -> Result<()> {
        let mut packet = PacketBuffer::new();
        
        // TODO: Update the last received packet counter (and verify that we haven't
        // received an old packet).

        executor::sleep(Duration::from_secs(1)).await?;

        loop {
            let data = shared.device.recv_radio_packet("rx_buffer").await?;
            packet.raw_mut()[0..data.len()].copy_from_slice(&data[..]);

            println!("RX {:?}", packet.remote_address());

            lock!(state <= shared.state.lock().await?, {

                if let Some(receiver) = state.receivers.get(packet.remote_address()) {
                    // TODO: Add the name to these?
                    let mut packet_proto = RadioBridgePacket::default();
                    packet_proto.set_data(packet.data());

                    // TODO: Issue a warning on overflow.
                    let _ = receiver.try_send(packet_proto);
                }
            });
        }
    }

    // NOTE: We only support 1 transmission at a time so this is its own single thread
    // to limit transmission concurrency.
    async fn tx_thread(
        shared: Arc<Shared>,
        event_receiver: channel::Receiver<()>,
    ) -> Result<()> {

        loop {

            let mut new_config = None;
            let mut next_packet = None;
            
            lock!(state <= shared.state.lock().await?, {
                if state.config_changed {
                    new_config = Some(state.state_data.network().clone());
                    state.config_changed = false;
                }
                
                if let Some(packet) = state.send_queue.pop() {
                    let address = Self::name_to_address(packet.device_name(), &state);
                    next_packet = Some((address, packet));
                }
            });

            if let Some(config) = new_config {
                shared.device.raw().set_network_config(&config).await?;
            }

            if let Some((address, packet)) = next_packet {
                let address = match address {
                    Some(addr) => addr,
                    // NOTE: If a device is removed shortly after a Send RPC, it may not be sent or
                    // return an error to the caller in this case.
                    None => continue,
                };

                println!("TX: {:?}", address);

                let mut packet_buffer = PacketBuffer::new();
                // packet_buffer.set_counter(self.next_packet_counter(&mut state).await?);
                packet_buffer.remote_address_mut().copy_from_slice(&address);
                packet_buffer.resize_data(packet.data().len());
                packet_buffer.data_mut().copy_from_slice(packet.data());

                shared.device.send_radio_packet("tx_buffer", packet_buffer.as_bytes()).await?;

                // Check if there are more packets to send.
                continue;
            }

            let _ = event_receiver.recv().await;
        }
    }

    fn name_to_address(name: &str, state: &State) -> Option<RadioAddress> {
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
                .set_object_proto(&state.state_object_name, &next_data)
                .await?;
            state.state_data = next_data;
        }

        state.last_packet_counter += 1;
        Ok(state.last_packet_counter)
    }
    */
}

#[async_trait]
impl RadioBridgeService for RadioBridge {
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

        if Self::name_to_address(request.device().name(), &state)
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
            .set_object_proto(&state.state_object_name, &next_data)
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
            .set_object_proto(&state.state_object_name, &state.state_data)
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
            if Self::name_to_address(request.device_name(), &state)
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
            let address = Self::name_to_address(request.device_name(), &state)
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
    bridge: &'a RadioBridge,
}

impl<'a> Drop for ReceiverRegistration<'a> {
    fn drop(&mut self) {
        let shared = self.bridge.shared.clone();
        let address = self.address.clone();
        executor::spawn(async move {
            lock!(state <= shared.state.lock().await.unwrap(), {
                state.receivers.remove(&address);
            });
        });
    }
}
