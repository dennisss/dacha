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
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::NetworkConfig;
use nordic_proto::usb::ProtocolUSBRequestType;
use protobuf::Message;
use usb::descriptors::{DescriptorType, SetupPacket, StandardRequestType};

use crate::log;
use crate::radio::Radio;
use crate::radio_socket::RadioSocket;
use crate::usb::controller::{USBDeviceControlRequest, USBDeviceControlResponse};
use crate::usb::default_handler::USBDeviceDefaultHandler;
use crate::usb::handler::USBDeviceHandler;

pub struct ProtocolUSBHandler {
    radio_socket: &'static RadioSocket,
    packet_buf: PacketBuffer,
}

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
    pub fn new(radio_socket: &'static RadioSocket) -> Self {
        Self {
            radio_socket,
            packet_buf: PacketBuffer::new(),
        }
    }

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) {
        if setup.bmRequestType == 0b01000000 {
            if setup.bRequest == ProtocolUSBRequestType::Send.to_value() {
                log!(b"\n");

                let n = req.read(self.packet_buf.raw_mut()).await;
                // TODO: Verify this doesn't crash due to the first byte being invalid causing
                // an out of bounds error.
                if n != self.packet_buf.as_bytes().len() {
                    return;
                }

                self.radio_socket.enqueue_tx(&mut self.packet_buf).await;

                return;
            } else if setup.bRequest == ProtocolUSBRequestType::SetNetworkConfig.to_value() {
                // TODO: Just re-use the same buffer as used for the packet?
                let mut raw_proto = [0u8; 256];
                let n = req.read(&mut raw_proto).await;

                log!(b"USB SET CFG\n");

                // log!(crate::num_to_slice(n as u32).as_ref());
                for i in 0..n {
                    log!(crate::num_to_slice(raw_proto[i] as u32).as_ref());
                    log!(b", ");
                }

                log!(b"\n");

                let proto = match NetworkConfig::parse(&raw_proto[0..n]) {
                    Ok(v) => v,
                    Err(e) => {
                        log!(b"PARSE FAIL\n");

                        return;
                    }
                };

                self.radio_socket.set_network_config(proto).await;

                log!(b"=> DONE\n");

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
                let has_data = self.radio_socket.dequeue_rx(&mut self.packet_buf).await;
                res.write(if has_data {
                    self.packet_buf.as_bytes()
                } else {
                    &[]
                })
                .await;
                return;
            } else if setup.bRequest == ProtocolUSBRequestType::GetNetworkConfig.to_value() {
                log!(b"USB GETCFG\n");

                let mut raw_proto = common::collections::FixedVec::new([0u8; 256]);

                let network_config = self.radio_socket.lock_network_config().await;
                if let Err(_) = network_config.get().serialize_to(&mut raw_proto) {
                    // TODO: Make sure this returns an error over USB?
                    log!(b"USB SER FAIL\n");
                    res.stale();
                    return;
                }

                drop(network_config);

                res.write(raw_proto.as_ref()).await;

                return;
            }
        }

        USBDeviceDefaultHandler::new()
            .handle_control_response(setup, res)
            .await
    }
}
