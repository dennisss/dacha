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

use common::attribute::GetAttributeValue;
use common::errors::*;
use common::fixed::vec::FixedVec;
use common::list::Appendable;
use executor::channel::Channel;
use executor::futures::*;
use logging::Logger;
use nordic_proto::nordic::NetworkConfig;
use nordic_wire::packet::PacketBuffer;
use nordic_wire::request_type::ProtocolRequestType;
use protobuf::{Message, StaticMessage};
use usb::descriptors::{DescriptorType, SetupPacket, StandardRequestType};

use crate::radio::Radio;
use crate::radio_socket::RadioSocket;
use crate::timer::Timer;
use crate::usb::controller::{
    USBDeviceControlRequest, USBDeviceControlResponse, USBDeviceController, USBDeviceNormalRequest,
};
use crate::usb::default_handler::USBDeviceDefaultHandler;
use crate::usb::handler::{USBDeviceHandler, USBError};

pub trait ProtocolUSBDescriptorSet =
    usb::DescriptorSet + GetAttributeValue<usb::dfu::DFUInterfaceNumberTag> + Copy + 'static;

pub struct ProtocolUSBHandler<D> {
    descriptors: D,
    radio_socket: &'static RadioSocket,
    timer: Timer,
    packet_buf: PacketBuffer,
}

// TODO: Have a macro to auto-generate this.
impl<D: ProtocolUSBDescriptorSet> USBDeviceHandler for ProtocolUSBHandler<D> {
    type HandleControlRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalResponseAcknowledgedFuture<'a> =
        impl Future<Output = Result<(), USBError>> + 'a;

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

    fn handle_normal_request<'a>(
        &'a mut self,
        endpoint_index: usize,
        req: USBDeviceNormalRequest,
    ) -> Self::HandleNormalRequestFuture<'a> {
        async move { Ok(()) }
    }

    fn handle_normal_response_acknowledged<'a>(
        &'a mut self,
        endpoint_index: usize,
    ) -> Self::HandleNormalResponseAcknowledgedFuture<'a> {
        async move { Ok(()) }
    }
}

impl<D: ProtocolUSBDescriptorSet> ProtocolUSBHandler<D> {
    pub fn new(descriptors: D, radio_socket: &'static RadioSocket, timer: Timer) -> Self {
        Self {
            descriptors,
            radio_socket,
            timer,
            packet_buf: PacketBuffer::new(),
        }
    }

    // TODO: Add a 'FactoryReset' command which simply clears all in-volatile state
    // and resets the device.

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b01000000
        /* Host-to-device | Vendor | Device */
        {
            if setup.bRequest == ProtocolRequestType::Send.to_value() {
                log!("USB TX");

                let n = req.read(self.packet_buf.raw_mut()).await?;
                // TODO: Verify this doesn't crash due to the first byte being invalid causing
                // an out of bounds error.
                // Must be at least large enough to fit all auxiliary fields.
                // Must be
                if n != self.packet_buf.as_bytes().len() {
                    return Ok(());
                }

                let _ = self.radio_socket.enqueue_tx(&mut self.packet_buf).await;

                return Ok(());
            } else if setup.bRequest == ProtocolRequestType::SetNetworkConfig.to_value() {
                // TODO: Just re-use the same buffer as used for the packet?
                let mut raw_proto = [0u8; 256];
                let n = req.read(&mut raw_proto).await?;

                log!("USB SET CFG");

                let proto = match NetworkConfig::parse(&raw_proto[0..n]) {
                    Ok(v) => v,
                    Err(e) => {
                        log!("PARSE FAIL");

                        return Ok(());
                    }
                };

                // Ignore errors.
                let _ = self.radio_socket.set_network_config(proto).await;

                log!("=> DONE");

                return Ok(());
            }
        }

        // On DFU_DETACH resets, reset to the bootloader.
        if setup.bmRequestType == 0b00100001
        /* Host-to-device | Class | Interface */
        {
            if setup.wIndex == get_attr!(&self.descriptors, usb::dfu::DFUInterfaceNumberTag) as u16
                && setup.bRequest == usb::dfu::DFURequestType::DFU_DETACH as u8
            {
                req.read(&mut []).await?;

                // Give the application enough time to notice the response.
                self.timer.wait_ms(10).await;

                crate::reset::reset_to_bootloader();
            }
        }

        USBDeviceDefaultHandler::new(self.descriptors)
            .handle_control_request(setup, req)
            .await
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b11000000
        /* Device-to-host | Vendor | Device */
        {
            if setup.bRequest == ProtocolRequestType::Receive.to_value() {
                // log!("USB RX");
                let has_data = self.radio_socket.dequeue_rx(&mut self.packet_buf).await;
                res.write(if has_data {
                    self.packet_buf.as_bytes()
                } else {
                    &[]
                })
                .await?;
                return Ok(());
            } else if setup.bRequest == ProtocolRequestType::GetNetworkConfig.to_value() {
                log!("USB GETCFG");

                let mut raw_proto = common::fixed::vec::FixedVec::<u8, 256>::new();

                let network_config = self.radio_socket.lock_network_config().await;
                if let Some(network_config) = network_config.get() {
                    if let Err(_) = network_config.serialize_to(&mut raw_proto) {
                        // TODO: Make sure this returns an error over USB?
                        log!("USB SER FAIL");
                        res.stale();
                        return Ok(());
                    }
                }

                drop(network_config);

                res.write(raw_proto.as_ref()).await?;

                return Ok(());
            } else if setup.bRequest == ProtocolRequestType::ReadLog.to_value() {
                let mut buffer = [0u8; 256];
                if (setup.wLength as usize) < buffer.len() {
                    res.stale();
                    return Ok(());
                }

                let mut n = 0;

                while n < buffer.len() {
                    if let Some(len) = Logger::global().try_read(&mut buffer[(n + 1)..]).await {
                        buffer[n] = len as u8;
                        n += len + 1;
                    } else {
                        break;
                    }
                }

                res.write(&buffer[0..n]).await?;
                return Ok(());
            }
        }

        USBDeviceDefaultHandler::new(self.descriptors)
            .handle_control_response(setup, res)
            .await
    }
}

pub async fn protocol_usb_thread_fn<D: ProtocolUSBDescriptorSet>(
    descriptors: D,
    mut usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    timer: Timer,
) {
    usb.run(ProtocolUSBHandler::new(descriptors, radio_socket, timer))
        .await;
}
