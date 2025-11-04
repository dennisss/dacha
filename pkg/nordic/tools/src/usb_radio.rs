use std::time::Duration;
use std::time::Instant;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};

use common::errors::*;
use common::bit_set::BitSet;
use common::hash::FastHasherBuilder;
use nordic_proto::nordic::*;
use nordic_wire::packet::PacketBuffer;
use nordic_wire::request_type::ProtocolRequestType;
use protobuf::{Message, StaticMessage};
use usb::{descriptors::SetupPacket, registry::OUR_VENDOR_ID};
use peripherals_proto::peripherals::*;
use executor::channel::oneshot;
use executor::lock;
use executor::sync::AsyncVariable;
use executor_multitask::{impl_resource_passthrough, TaskResource};


const MAX_ACTIVE_REQUESTS: usize = 128;

// NOTE: With the current implentation, we may end up sending a bit more than this.
const SEND_BUFFER_SIZE: usize = 64;


// TODO: Every single USB transfer should have some timeout.
pub struct USBRadio {
    shared: Arc<Shared>,
    task: TaskResource
}

impl_resource_passthrough!(USBRadio, task);

struct Shared {
    device: usb::Device,

    state: AsyncVariable<State>,
}

struct State {

    // TODO: Need to know if the background thread is still healthy (this also needs to be aware of whether the background thread got cancelled so couldn't mark itself as dead).

    connected: bool,

    /// Last sequence number used by a request added to send_queue.
    last_sequence: u32,

    /// Requests enqueud to be sent to the device by the background thread.
    send_queue: VecDeque<PeripheralRequest>,

    /// Map of sequence number to the channel to use to deliver the response
    /// for an active request.
    active_requests: HashMap<u32, oneshot::Sender<PeripheralResponse>, FastHasherBuilder>,
}

impl USBRadio {
    pub async fn find(device_selector: &usb::DeviceSelector) -> Result<Self> {
        let ctx = usb::Context::create()?;

        let mut device = {
            let mut found_device = None;

            for dev in ctx.enumerate_devices().await? {
                if !device_selector.matches(&dev)? {
                    continue;
                }

                let id = format!("{}.{}", dev.bus_num(), dev.dev_num());
                println!("Device: {}", id);

                found_device = Some(dev.open().await?);
            }

            found_device.ok_or_else(|| err_msg("No device selected"))?
        };

        println!("Device opened!");

        device.reset()?;
        println!("Device reset!");

        Ok(Self::new(device))
    }

    fn new(device: usb::Device) -> Self {

        let shared = Arc::new(Shared {
            device,
            state: AsyncVariable::new(State {
                connected: true,
                last_sequence: 0,
                send_queue: VecDeque::new(),
                active_requests: HashMap::default()
            })
        });

        let guard = ConnectionGuard { shared: shared.clone() };

        let task = TaskResource::spawn_interruptable("USBRadio", Self::background_thread(guard, shared.clone()));

        Self {
            shared,
            task
        }
    }

    async fn background_thread(
        guard: ConnectionGuard,
        shared: Arc<Shared>
    ) -> Result<()> {
        loop {
            let mut send_buffer = vec![];
            let mut have_active_requests = false;

            lock!(state <= shared.state.lock().await?, {
                while send_buffer.len() < SEND_BUFFER_SIZE {
                    let request = match state.send_queue.pop_front() {
                        Some(v) => v,
                        None => break
                    };

                    let proto = request.serialize()?;
                    send_buffer.push(proto.len() as u8);
                    send_buffer.extend_from_slice(&proto);
                } 

                have_active_requests = !state.active_requests.is_empty();

                Result::<_, Error>::Ok(())
            })?;

            if !send_buffer.is_empty() {
                // TODO: For whatever reason, if the packet is some specific sizes (e.g. 9),
                // then the nordic controller just stalls.
                if send_buffer.len() < SEND_BUFFER_SIZE {
                    send_buffer.resize(SEND_BUFFER_SIZE, 0);
                }

                // TODO: Support retrying this (must consider the idempotence of actions).
                shared.device
                    .write_control(
                        SetupPacket {
                            bmRequestType: 0b01000000,
                            bRequest: ProtocolRequestType::PeripheralRequest.to_value(),
                            wValue: 0,
                            wIndex: 0,
                            wLength: send_buffer.len() as u16,
                        },
                        &send_buffer,
                    )
                    .await?;
            }

            let mut res_buffer = [0u8; 256];

            if have_active_requests {
                // Attempt to RX.

                loop {
                    let nread = shared
                        .device
                        .read_control(
                            SetupPacket {
                                bmRequestType: 0b11000000,
                                bRequest: ProtocolRequestType::PeripheralResponse.to_value(),
                                wValue: 0,
                                wIndex: 0,
                                wLength: res_buffer.len() as u16,
                            },
                            &mut res_buffer,
                        )
                        .await?;

                    if nread == 0 {
                        break;
                    }

                    let response = PeripheralResponse::parse(&res_buffer[0..nread])?;

                    lock!(state <= shared.state.lock().await?, {
                        let sender = state.active_requests.remove(&response.request_sequence())
                            .ok_or_else(|| format_err!("No active request for response with sequence: {}", response.request_sequence()))?;

                        let _ = sender.send(response);
                        Result::<_, Error>::Ok(())
                    })?;
                }
            }

            // Wait either 10ms or for more requests to be available to send.
            {
                let state = shared.state.lock().await?.read_exclusive();
                if !state.send_queue.is_empty() {
                    continue;
                }

                executor::timeout(Duration::from_millis(10), state.wait()).await;
            }
        }
    }

    pub async fn get_clock_time(&self) -> Result<u32> {
        let mut buf = [0u8; 4];
        let n = self.shared
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolRequestType::GetClockTime.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: buf.len() as u16,
                },
                &mut buf,
            )
            .await?;

        if n != buf.len() {
            return Err(err_msg("Did not read a full u32"));
        }

        Ok(u32::from_le_bytes(buf))
    }

    pub async fn set_network_config(&mut self, config: &NetworkConfig) -> Result<()> {
        let proto = config.serialize()?;
        self.shared.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b01000000,
                    bRequest: ProtocolRequestType::SetNetworkConfig.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: proto.len() as u16,
                },
                &proto,
            )
            .await?;
        Ok(())
    }

    pub async fn get_network_config(&mut self) -> Result<Option<NetworkConfig>> {
        let mut read_buffer = [0u8; 256];
        // TODO: Set a timeout on this and reset the device on failure.
        let n = self
            .shared
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolRequestType::GetNetworkConfig.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: read_buffer.len() as u16,
                },
                &mut read_buffer,
            )
            .await?;

        if n == 0 {
            return Ok(None);
        }

        Ok(Some(NetworkConfig::parse(&read_buffer[0..n])?))
    }

    pub async fn send_packet(&mut self, packet: &PacketBuffer) -> Result<()> {
        // TODO: Support retrying this (must consider the idempotence of actions).
        self.shared.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b01000000,
                    bRequest: ProtocolRequestType::Send.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: packet.as_bytes().len() as u16,
                },
                packet.as_bytes(),
            )
            .await?;

        Ok(())
    }

    /// NOTE: Does not block if a packet isn't currently available.
    pub async fn recv_packet(&mut self) -> Result<Option<PacketBuffer>> {
        let mut packet_buffer = PacketBuffer::new();

        let mut num_bytes = None;
        for attempt in 0..4 {
            match executor::timeout(
                Duration::from_millis(5),
                self.shared.device.read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: ProtocolRequestType::Receive.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: packet_buffer.raw_mut().len() as u16,
                    },
                    packet_buffer.raw_mut(),
                ),
            )
            .await
            {
                Ok(Ok(n)) => {
                    num_bytes = Some(n);
                    break;
                }
                Err(_) => {
                    // Timeout
                    println!("Retrying read_control {}", attempt);
                    continue;
                }

                Ok(Err(e)) => {
                    // Internal USB error
                    return Err(e);
                }
            }
        }

        let num_bytes = num_bytes.ok_or_else(|| err_msg("Ran out of USB retries"))?;

        if num_bytes > 0 {
            Ok(Some(packet_buffer))
        } else {
            Ok(None)
        }
    }

    pub async fn read_log_entries(&mut self) -> Result<Vec<LogEntry>> {
        let mut buffer = [0u8; 256];
        let n = self
            .shared
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolRequestType::ReadLog.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: buffer.len() as u16,
                },
                &mut buffer,
            )
            .await?;

        let mut out = vec![];

        let mut i = 0;
        while i < n {
            let len = buffer[i] as usize;
            i += 1;

            if i + len > n {
                return Err(err_msg("Log entry larger than buffer length"));
            }

            let data = &buffer[i..(i + len)];
            i += len;

            out.push(LogEntry::parse(data)?);
        }

        Ok(out)
    }

    /// Issues a PeripheralRequest to the device and waits for the response.
    ///
    /// Errors in the PeriheralResponse will be raised as Result::Error.
    ///
    /// NOTE: In order for this future to be cancellable, the request is enqueued
    /// for the background thread to sent it rather than sending it immediately.
    /// This ensures that we don't get into a situation where we sent a request
    /// but then immediately forget that we sent it.
    pub async fn send_request(
        &self,
        request: &PeripheralRequest,
    ) -> Result<PeripheralResponse> {

        let (sender, receiver) = oneshot::channel();

        lock!(state <= self.shared.state.lock().await?, {
            // TODO: Check that the background thread is still healthy.

            if !state.connected {
                return Err(err_msg("Device is already disconnected"));
            }

            if state.active_requests.len() == MAX_ACTIVE_REQUESTS {
                return Err(err_msg("Too much active requests to device"));
            }

            // Acquire the next unused sequence
            // This is guaranteed to terminate since 'active_requests' isn't large
            // enough to occupy all slots. 
            loop {
                state.last_sequence += 1;
                if state.last_sequence == (MAX_ACTIVE_REQUESTS as u32) + 1 {
                    state.last_sequence = 1;
                }

                if !state.active_requests.contains_key(&state.last_sequence) {
                    break;
                }
            }

            let seq = state.last_sequence;
            let mut request = request.clone();
            request.set_request_sequence(seq);

            state.send_queue.push_back(request);
            state.active_requests.insert(seq, sender);

            state.notify_all();

            Result::<_, _>::Ok(())
        })?;

        let response = receiver.recv().await
            .map_err(|_| err_msg("Device background thread failed"))?;

        if response.error_code() != PeripheralResponse_ErrorCode::NO_ERROR {
            // TODO: Use an inline formatter for the request.
            return Err(format_err!("Request '{:?}' failed with code: {:?}", request, response.error_code()));
        }

        Ok(response)
    }
}

struct ConnectionGuard {
    shared: Arc<Shared>
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        executor::spawn(async move {
            let state = match shared.state.lock().await {
                Ok(v) => v,
                Err(_) => return
            };

            lock!(state <= state, {
                state.connected = false;
                // Notifies all waiters on the channels to wake up.
                state.active_requests.clear();
            });
        });
    }

}

