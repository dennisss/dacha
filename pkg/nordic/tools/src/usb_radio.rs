use std::time::Duration;
use std::time::Instant;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use std::future::Future;

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
use executor::bundle::TaskResultBundle;

const MAX_ACTIVE_REQUESTS: usize = 128;

const MAX_PACKET_SIZE: usize = 64;

/// Timeout on a single USB transaction.
const USB_TIMEOUT: Duration = Duration::from_millis(10000);


#[derive(Clone, Debug)]
pub struct ClockTimeResponse {
    pub remote_time: u32,
    pub local_request_time: Instant,
    pub local_response_time: Instant,
}


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
    /// Becomes false when the background thread dies indicating that we can no longer
    /// talk to the device.
    connected: bool,

    /// Last sequence number used by a request added to send_queue.
    last_sequence: u32,

    last_id: u64,

    /// Requests enqueud to be sent to the device by the background thread.
    /// TODO: Flatten this and have senders directly serialize in a buffer that is swapped out
    /// when the background thread is ready to send stuff.
    send_queue: VecDeque<SendQueueEntry>,

    high_priority_send_queue: VecDeque<SendQueueEntry>,

    /// Map of sequence number to the channel to use to deliver the response
    /// for an active request.
    active_requests: HashMap<u32, ActiveRequestsEntry, FastHasherBuilder>,
}

struct SendQueueEntry {
    data: Vec<u8>,
    sequence: u32,
    id: u64,
    cancellation: bool,
}

struct ActiveRequestsEntry {
    id: u64,
    sender: oneshot::Sender<USBRadioRequestResponse>,
    send_time: Option<Instant>,
}

pub struct USBRadioRequestResponse {
    pub res: PeripheralResponse,
    pub send_time: Instant,
    pub receive_time: Instant
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

        device.claim_interface(0)?;

        // TODO: Before handling it over, do an initial round of receiving any data and make sure the receiver buffer is clear.

        Ok(Self::new(device))
    }

    fn new(device: usb::Device) -> Self {

        let shared = Arc::new(Shared {
            device,
            state: AsyncVariable::new(State {
                connected: true,
                last_sequence: 0,
                last_id: 0,
                send_queue: VecDeque::new(),
                high_priority_send_queue: VecDeque::new(),
                active_requests: HashMap::default(),
            })
        });

        let guard = ConnectionGuard { shared: shared.clone() };

        let task = TaskResource::spawn_interruptable("USBRadio", Self::background_thread(guard, shared.clone()));

        Self {
            shared,
            task
        }
    }

    async fn control_write(shared: &Shared, request_type: ProtocolRequestType, data: &[u8]) -> Result<()> {
        executor::timeout(
            USB_TIMEOUT,
            shared.device
                .write_control(
                    SetupPacket {
                        bmRequestType: 0b01000000,
                        bRequest: request_type.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: data.len() as u16,
                    },
                    data,
                )
        )
        .await
        .map_err(|_| err_msg("Timeout during write_control"))?
    }

    async fn control_read(shared: &Shared, request_type: ProtocolRequestType, data: &mut [u8]) -> Result<usize> {
        executor::timeout(
            USB_TIMEOUT,
            shared.device
                .read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: request_type.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: data.len() as u16,
                    },
                    data,
                )
        ).await.map_err(|_| err_msg("Timeout during read_control"))?
    }

    async fn background_thread(
        guard: ConnectionGuard,
        shared: Arc<Shared>
    ) -> Result<()> {
        let mut bundle = TaskResultBundle::new();
        bundle.add("Sender", Self::sending_thread(shared.clone()));
        bundle.add("Receier", Self::receiver_thread(shared.clone()));
        bundle.join().await
    }

    async fn sending_thread(shared: Arc<Shared>) -> Result<()> {
        let mut send_buffer = vec![];
        send_buffer.reserve_exact(MAX_PACKET_SIZE);

        loop {
            // Send all available requests of batches of requests of size SEND_BUFFER_SIZE.
            loop {
                send_buffer.clear();

                lock!(state <= shared.state.lock().await?, {
                    
                    let now = Instant::now();

                    while send_buffer.len() < MAX_PACKET_SIZE {
                        let (is_high_pri, request) = match state.high_priority_send_queue.front() {
                            Some(v) => {
                                (true, v)
                            },
                            None => {
                                match state.send_queue.front() {
                                    Some(v) => (false, v),
                                    None => break
                                }
                            }
                        };


                        // TODO: Compress requests and don't send requests with consecutive sequences.

                        if request.data.len() == 0 {
                            return Err(err_msg("Empty request"));
                        }

                        if send_buffer.len() + 1 + request.data.len() > MAX_PACKET_SIZE {
                            if send_buffer.len() == 0 {
                                return Err(err_msg("Request too big to fit in send buffer."));
                            }

                            break;
                        }

                        send_buffer.push(request.data.len() as u8);
                        send_buffer.extend_from_slice(&request.data);
                        
                        // TODO: Check if still in active_requests earlier since the response may have
                        // been received by the time we got ready to send the cancellation.
                        if !request.cancellation {
                            let seq = request.sequence;

                            state.active_requests.get_mut(&seq).unwrap()
                                .send_time = Some(now);
                        }
                        
                        if is_high_pri {
                            state.high_priority_send_queue.pop_front();
                        } else {
                            state.send_queue.pop_front();
                        }

                    } 

                    Result::<_, Error>::Ok(())
                })?;

                if !send_buffer.is_empty() {
                    // The nRF52 is really bad with odd numbers of bytes or non-multiples of 4 so
                    // add some padding for stability.
                    send_buffer.resize(send_buffer.len().next_multiple_of(4), 0);

                    // TODO: Need to support enqueuing multiple packets at once.
                    executor::timeout(
                        USB_TIMEOUT,
                        shared.device.write_bulk(0x02, &send_buffer)
                    )
                    .await
                    .map_err(|_| err_msg("Timeout while doing write_bulk"))??;

                    continue;
                }

                break;
            }

            // Wait either 10ms or for more requests to be available to send.
            {
                let state = shared.state.lock().await?.read_exclusive();
                if !state.send_queue.is_empty() || !state.high_priority_send_queue.is_empty() {
                    continue;
                }

                executor::timeout(Duration::from_millis(100), state.wait()).await;
            }
        }
    }

    async fn receiver_thread(shared: Arc<Shared>) -> Result<()> {
        // This is the max size of one USB packet. Note that the device doesn't send
        // ZLPs so we can't make this bigger (since it won't work correctly if exactly
        // this many bytes are transfered).
        const CHUNK_SIZE: usize = 64;
        
        let mut res_buffer = vec![];

        // TODO: Vec dequeue.
        let mut read_queue = vec![];

        loop {
            while read_queue.len() < 4 {
                read_queue.push(shared.device.enqueue_read_bulk(0x81, CHUNK_SIZE)?);
            }

            let read = read_queue.remove(0).wait().await?;

            // TODO: Need per-byte receive times since a response may be split across
            // one or more packets.
            let receive_time = Instant::now();

            let original_buffer_len = res_buffer.len();
            let nread = read.buffer().len();
            res_buffer.resize(original_buffer_len + nread, 0);
            res_buffer[original_buffer_len..].copy_from_slice(read.buffer());

            // The device will always fill the whole buffer if it has enough data.
            // So if the buffer wasn't filled, there is nothing else to read for now.
            let mut done = nread < CHUNK_SIZE;

            let mut i = 0;
            while i < res_buffer.len() {
                let len = res_buffer[i] as usize;
                
                // Zero length responses are treated as padding and also signal that the device
                // didn't have more data to send.
                if len == 0 {
                    // Main exception is that if the device sends back a packet with nothing but
                    // zeroes, then its a signal that we overflowed the response buffer.
                    if i == original_buffer_len {
                        return Err(err_msg("MCU buffer overflowed"));
                    }

                    done = true;
                    i = res_buffer.len();
                    break;
                }

                if i + 1 + len > res_buffer.len() {
                    if done {
                        return Err(err_msg("Done but expecting more data immediately"));
                    }

                    break;
                }

                i += 1;
                let response = PeripheralResponse::parse(&res_buffer[i..(i + len)])?;
                i += len;

                // TODO: Maybe lock this once for the entire parsing part (maybe after doing parsing and we have the responses in a vec).
                lock!(state <= shared.state.lock().await?, {
                    for batch_i in 0..(response.ack_next_n() + 1) {
                        let seq = response.request_sequence() + batch_i;

                        let active_req = state.active_requests.remove(&seq)
                            .ok_or_else(|| format_err!("No active request for response with sequence: {}. {:?}", seq, response))?;

                        let _ = active_req.sender.send(USBRadioRequestResponse {
                            res: response.clone(),
                            send_time: active_req.send_time.clone().unwrap(),
                            receive_time: receive_time
                        });
                    }

                    Result::<_, Error>::Ok(())
                })?;
            }

            // Clean up read bytes
            // Note that this fairly cheap (bounded by CHUNK_SIZE)
            res_buffer.drain(..i);
        }
    }

    pub async fn get_clock_time(&self) -> Result<ClockTimeResponse> {

        let mut req = PeripheralRequest::default();
        req.set_get_clock_time(true);

        // Using the high priority queue.
        let res = self.enqueue_request_batch_inner(&[req], true).await?.await?;
        let res = &res[0];

        let remote_time = res.res.uint_val();
        Ok(ClockTimeResponse {
            remote_time,
            local_request_time: res.send_time,
            local_response_time: res.receive_time
        })
    }

    pub async fn get_idle_counter(&self) -> Result<u32> {
        let mut req = PeripheralRequest::default();
        req.set_get_idle_counter(true);

        let res = self.send_request(&req).await?;
        Ok(res.uint_val())
    }

    pub async fn set_network_config(&mut self, config: &NetworkConfig) -> Result<()> {
        let proto = config.serialize()?;
        Self::control_write(
            &self.shared,
            ProtocolRequestType::SetNetworkConfig,
            &proto
        ).await?;
        Ok(())
    }

    pub async fn get_network_config(&mut self) -> Result<Option<NetworkConfig>> {
        let mut read_buffer = [0u8; 256];
        // TODO: Set a timeout on this and reset the device on failure.

        let n = Self::control_read(
            &self.shared,
            ProtocolRequestType::GetNetworkConfig,
            &mut read_buffer
        ).await?;

        if n == 0 {
            return Ok(None);
        }

        Ok(Some(NetworkConfig::parse(&read_buffer[0..n])?))
    }

    pub async fn send_packet(&mut self, packet: &PacketBuffer) -> Result<()> {
        // TODO: Support retrying this (must consider the idempotence of actions).
        Self::control_write(
            &self.shared,
            ProtocolRequestType::Send,
            packet.as_bytes()
        ).await?;
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
        let n = Self::control_read(
            &self.shared,
            ProtocolRequestType::ReadLog,
            &mut buffer
        ).await?;

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

    /*
    TODO: Only assign sequences right before sending.
    - Then we can do sequence compression and support having prioritized requests
    to jump the queue without penalties.
    */

    pub async fn enqueue_request<'a>(
        &'a self,
        request: &PeripheralRequest,
    ) -> Result<impl Future<Output = Result<PeripheralResponse>> + 'a> {
        let mut results = self.enqueue_request_batch(core::slice::from_ref(request)).await?;
        Ok(async move {
            Ok(results.await?.pop().unwrap())
        })
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
        let mut results = self.send_request_batch(core::slice::from_ref(request)).await?;
        Ok(results.pop().unwrap())
    }

    pub async fn enqueue_request_batch<'a>(
        &'a self, requests: &[PeripheralRequest]
    ) -> Result<impl Future<Output = Result<Vec<PeripheralResponse>>> + 'a> {
        let f = self.enqueue_request_batch_inner(requests, false).await?;

        Ok(async move {
            let res = f.await?;
            Ok(res.into_iter().map(|r| r.res).collect::<Vec<_>>())
        })
    }

    pub async fn enqueue_request_batch_inner<'a>(
        &'a self, requests: &[PeripheralRequest], high_priority: bool,
    ) -> Result<impl Future<Output = Result<Vec<USBRadioRequestResponse>>> + 'a> {

        let requests = requests.to_vec();

        let mut senders = vec![];
        let mut receivers = vec![];
        senders.reserve_exact(requests.len());
        receivers.reserve_exact(requests.len());

        for _ in 0..requests.len() {
            let (s, r) = oneshot::channel();
            senders.push(s);
            receivers.push(r);
        }

        let mut request_guard = lock!(state <= self.shared.state.lock().await?, {
            // TODO: Check that the background thread is still healthy.

            if !state.connected {
                return Err(err_msg("Device is already disconnected"));
            }

            if state.active_requests.len() + requests.len() > MAX_ACTIVE_REQUESTS {
                return Err(err_msg("Too much active requests to device"));
            }

            let mut pending_requests = vec![];
            pending_requests.reserve_exact(requests.len());

            for (request, sender) in requests.iter().zip(senders.into_iter()) {
                let id = state.last_id + 1;
                state.last_id = id;
                
                // Acquire the next unused sequence
                // This is guaranteed to terminate since 'active_requests' isn't large
                // enough to occupy all slots. 
                common::loops::bounded_loop(MAX_ACTIVE_REQUESTS, || {
                    state.last_sequence += 1;
                    if state.last_sequence == (MAX_ACTIVE_REQUESTS as u32) + 1 {
                        state.last_sequence = 1;
                    }

                    if !state.active_requests.contains_key(&state.last_sequence) {
                        return Ok(common::loops::Loop::Break);
                    }

                    Ok(common::loops::Loop::Continue)
                })?;

                let seq = state.last_sequence;
                let mut request = request.clone();
                request.set_request_sequence(seq);
                pending_requests.push(PendingRequest {
                    id,
                    sequence: seq,
                    peripheral_index: request.peripheral_index()
                });

                // TODO: Ideally serialize outside of the lock.
                let send_entry = SendQueueEntry {
                    data: request.serialize()?,
                    id,
                    sequence: seq,
                    cancellation: false,
                };

                if high_priority {
                    state.high_priority_send_queue.push_back(send_entry);
                } else {
                    state.send_queue.push_back(send_entry);
                }

                state.active_requests.insert(seq, ActiveRequestsEntry {
                    sender,
                    id,
                    send_time: None,
                });
            }

            state.notify_all();

            Result::<_, _>::Ok(RequestGuard {
                shared: self.shared.clone(),
                pending_requests: Some(pending_requests),
            })
        })?;

        // tODO: If any of this is dropped, we should isue request cancellations.

        Ok(async move {
            let mut responses = vec![];

            for (i, receiver) in receivers.into_iter().enumerate() {
                let response = receiver.recv().await
                    .map_err(|_| err_msg("Device background thread failed"))?;

                if response.res.error_code() != PeripheralResponse_ErrorCode::NO_ERROR {
                    // TODO: Use an inline formatter for the request.
                    return Err(format_err!("Request '{:?}' failed with code: {:?}", requests[i], response.res.error_code()));
                }

                responses.push(response);
            }

            // All responses were received, so no need to cancel anything.
            request_guard.pending_requests = None;
            drop(request_guard);

            Ok(responses)
        })
    }

    pub async fn send_request_batch(
        &self, requests: &[PeripheralRequest]
    ) -> Result<Vec<PeripheralResponse>> {
        self.enqueue_request_batch(requests).await?.await
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
                state.high_priority_send_queue.clear();
            });
        });
    }
}

struct RequestGuard {
    shared: Arc<Shared>,
    pending_requests: Option<Vec<PendingRequest>>
}

struct PendingRequest {
    sequence: u32,
    peripheral_index: u32,
    id: u64,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        let pending_requests = match self.pending_requests.take() {
            Some(v) => v,
            None => return
        };

        let shared = self.shared.clone();
        executor::spawn(async move {
            let state = match shared.state.lock().await {
                Ok(v) => v,
                Err(_) => return
            };

            lock!(state <= state, {
                for entry in pending_requests {
                    let active_req = match state.active_requests.get(&entry.sequence) {
                        Some(v) => v,
                        None => continue
                    };

                    if active_req.id != entry.id {
                        continue;
                    }

                    let mut cancel_req = PeripheralRequest::default();
                    cancel_req.set_peripheral_index(entry.peripheral_index);
                    cancel_req.set_request_sequence(entry.sequence);
                    cancel_req.set_cancel(true);

                    state.send_queue.push_back(SendQueueEntry {
                        data: cancel_req.serialize().unwrap(),
                        sequence: entry.sequence,
                        id: entry.id,
                        cancellation: true
                    });
                }
            });
        });
    }
}

