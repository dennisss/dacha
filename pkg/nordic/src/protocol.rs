/*
Commands to support:
- GetRadioConfig : May return an empty packet if
    - Returns a NetworkConfig proto serialized
- SetRadioConfig : Payload is a RadioConfig proto.
    - Takes as input a NetworkConfig proto
    - For now, we'll just store it in RAM

- Send
- Receive


Over USB:
    - bmRequestType:
        0bX10 00000 (Vendor request to/from Device)

    - bRequest:
        - 1: Send : Payload is just bytes to send
        - 2: Receive : Payload is the data recieved.


Device mode:
    - If not transmitting, we will be receiving.
    - Received data will go into a circular buffer from which the USB reads.


Threads:
1. Handling radio state
2. Handling USB stuff

Syncronization
- Radio thread is always waiting on either:
    - Receiving new packet over air
    - Waiting for something to be available to transfer

- We have two channels




*/

use core::future::Future;

use common::collections::FixedVec;
use common::list::Appendable;
use executor::channel::Channel;
use executor::futures::*;
use executor::mutex::Mutex;
use usb::descriptors::{DescriptorType, SetupPacket, StandardRequestType};

use crate::log;
use crate::radio::Radio;
use crate::usb::controller::{USBDeviceControlRequest, USBDeviceControlResponse};
use crate::usb::default_handler::USBDeviceDefaultHandler;
use crate::usb::handler::USBDeviceHandler;

/// Messages are sent on this channel from the USB thread to the Radio thread
/// when there is data present in the transmit buffer to be sent.
static TRANSMIT_PENDING: Channel<()> = Channel::new();

/// Size to use for all buffers. This is also the maximum size that we will
/// transmit or receive in one transaction.
const BUFFER_SIZE: usize = 64;

static TRANSMIT_BUFFER: Mutex<FixedVec<u8, [u8; BUFFER_SIZE]>> =
    Mutex::new(FixedVec::new([0u8; BUFFER_SIZE]));
// TODO: Change this is a cyclic buffer.
static RECEIVE_BUFFER: Mutex<FixedVec<u8, [u8; BUFFER_SIZE]>> =
    Mutex::new(FixedVec::new([0u8; BUFFER_SIZE]));

// /// Messages are sent on this channel from the Radio to USB threads
// static RECEIVE_BUFFER_READY: Channel<()> = Channel::new();

enum_def_with_unknown!(ProtocolUSBRequestType u8 =>
    Send = 1,
    Receive = 2
);

pub struct ProtocolUSBHandler {}

// TODO: Have a macro to auto-generate this.
impl USBDeviceHandler for ProtocolUSBHandler {
    type HandleControlRequestFuture<'a> = impl Future<Output = ()> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = ()> + 'a;

    fn handle_control_request<'a>(
        &'a mut self,
        setup: SetupPacket,
        req: USBDeviceControlRequest<'a>,
    ) -> Self::HandleControlRequestFuture<'a> {
        self.handle_control_request_impl(setup, req)
    }

    fn handle_control_response<'a>(
        &'a mut self,
        setup: SetupPacket,
        res: USBDeviceControlResponse<'a>,
    ) -> Self::HandleControlResponseFuture<'a> {
        self.handle_control_response_impl(setup, res)
    }
}

impl ProtocolUSBHandler {
    pub fn new() -> Self {
        Self {}
    }

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) {
        if setup.bmRequestType == 0b01000000 {
            if setup.bRequest == ProtocolUSBRequestType::Send.to_value() {
                log!(b"USB TX");

                let mut buffer = TRANSMIT_BUFFER.lock().await;
                buffer.resize(BUFFER_SIZE, 0);

                let n = req.read(buffer.as_mut()).await;
                buffer.truncate(n);

                log!(b"- READ!");

                drop(buffer);

                TRANSMIT_PENDING.send(()).await;

                return;
            }
        }

        USBDeviceDefaultHandler::new()
            .handle_control_request(setup, req)
            .await
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) {
        if setup.bmRequestType == 0b11000000 {
            if setup.bRequest == ProtocolUSBRequestType::Receive.to_value() {
                log!(b"USB RX\n");

                let mut buffer = RECEIVE_BUFFER.lock().await;
                res.write(buffer.as_ref()).await;
                buffer.clear();
                return;
            }
        }

        USBDeviceDefaultHandler::new()
            .handle_control_response(setup, res)
            .await
    }
}

pub struct ProtocolRadioThread {
    radio: Radio,
}

impl ProtocolRadioThread {
    pub fn new(radio: Radio) -> Self {
        Self { radio }
    }

    pub async fn run(mut self) {
        enum Event {
            Received(usize),
            TransmitPending,
        }

        let mut temp_buffer = [0u8; 64];

        loop {
            // TODO: Implement a more efficient way to cancel the receive future.
            let event = race2(
                map(self.radio.receive(&mut temp_buffer), |n| Event::Received(n)),
                map(TRANSMIT_PENDING.recv(), |_| Event::TransmitPending),
            )
            .await;

            match event {
                Event::Received(n) => {
                    log!(b"RADIO RX\n");

                    let mut rx_buffer = RECEIVE_BUFFER.lock().await;
                    rx_buffer.clear();
                    rx_buffer.extend_from_slice(&temp_buffer[0..n]);
                }
                Event::TransmitPending => {
                    log!(b"RADIO TX\n");

                    let mut tx_buffer = TRANSMIT_BUFFER.lock().await;
                    self.radio.send(&tx_buffer).await;
                }
            }
        }
    }
}
