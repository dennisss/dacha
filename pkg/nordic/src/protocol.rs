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

use base_util::aligned::Aligned;
use common::attribute::GetAttributeValue;
use common::errors::*;
use common::fixed::vec::FixedVec;
use common::list::Appendable;
use executor::channel::Channel;
use executor::futures::*;
use logging::Logger;
use nordic_proto::nordic::NetworkConfig;
use peripherals_proto::peripherals::PeripheralRequest;
use nordic_wire::packet::PacketBuffer;
use nordic_wire::request_type::ProtocolRequestType;
use protobuf::{Message, StaticMessage};
use usb::descriptors::{DescriptorType, SetupPacket, StandardRequestType};

use crate::controller::PeripheralsController;
use crate::radio::Radio;
use crate::radio_socket::RadioSocket;
use crate::rtc::RTC;
use crate::usb::controller::{
    USBDeviceControlRequest, USBDeviceControlResponse, USBDeviceController, USBDeviceNormalRequest, USBDeviceNormalResponse,
};
use crate::usb::default_handler::USBDeviceDefaultHandler;
use crate::usb::handler::{USBDeviceHandler, USBError};

pub trait ProtocolUSBDescriptorSet =
    usb::DescriptorSet + GetAttributeValue<usb::dfu::DFUInterfaceNumberTag> + Copy + 'static;

pub struct ProtocolUSBHandler<D> {
    descriptors: D,
    radio_socket: &'static RadioSocket,
    peripherals_controller: Option<&'static PeripheralsController>,
    rtc: RTC,
    packet_buf: PacketBuffer,
}

/*
Want to have a very fancy buffer for receiving requests:
- 256 bytes
- smart enough to amortize when to copy down bytes.
- same exact code as in the USBRadio ideally.


*/

// TODO: Have a macro to auto-generate this.
impl<D: ProtocolUSBDescriptorSet> USBDeviceHandler for ProtocolUSBHandler<D> {
    type HandleResetFuture<'a> = impl Future<Output = ()> + 'a;

    type HandleControlRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type PollNormalResponseReadyFuture<'a> = impl Future<Output = ()> + 'a;

    fn handle_reset<'a>(
        &'a mut self,
    ) -> Self::HandleResetFuture<'a> {
        async move {
            if let Some(controller) = &self.peripherals_controller {
                return controller.handle_reset().await;
            }

            ()
        }
    }

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
        mut req: USBDeviceNormalRequest<'a>,
    ) -> Self::HandleNormalRequestFuture<'a> {
        async move {
            // TODO: Decouple the buffer from being 'packet' typed and length.
            let mut raw_proto = self.packet_buf.raw_mut();
            let n = req.read_aligned(raw_proto).await?;
            self.process_peripheral_request_data(&self.packet_buf.raw()[0..n]).await;

            Ok(())
        }
    }

    fn handle_normal_response<'a>(
        &'a mut self,
        endpoint_index: usize,
        res: USBDeviceNormalResponse<'a>,
    ) -> Self::HandleNormalResponseFuture<'a> {
        self.handle_normal_response_impl(endpoint_index, res)
    }

    fn poll_normal_response_ready<'a>(
        &'a self,
        endpoint_index: usize,
    ) -> Self::PollNormalResponseReadyFuture<'a> {
        async move {
            if let Some(controller) = &self.peripherals_controller {
                return controller.wait_until_readable().await;
            }

            executor::futures::pending().await
        }
    }
}

impl<D: ProtocolUSBDescriptorSet> ProtocolUSBHandler<D> {
    pub fn new(
        descriptors: D,
        radio_socket: &'static RadioSocket,
        peripherals_controller: Option<&'static PeripheralsController>,
        rtc: RTC,
    ) -> Self {
        Self {
            descriptors,
            radio_socket,
            peripherals_controller,
            rtc,
            packet_buf: PacketBuffer::new(),
        }
    }

    async fn process_peripheral_request_data(&self, data: &[u8]) {
        let n = data.len();
        let mut i = 0;
        while i < n {
            let len = data[i] as usize;
            i += 1;

            if len == 0 {
                break;
            }

            if i + len > n {
                break;
            }

            let s = &data[i..(i + len)];
            i += len;

            let proto = match PeripheralRequest::parse(s) {
                Ok(v) => v,
                Err(e) => {
                    log!("PARSE FAIL");
                    return;
                }
            };

            // TODO: If we get no sequence, imply that it is the same as the last received request + 1

            if let Some(controller) = &self.peripherals_controller {
                controller.execute(&proto).await;
            }
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
                // TODO: Decouple the buffer from being 'packet' typed and length.
                let mut raw_proto = self.packet_buf.raw_mut();
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
            } else if setup.bRequest == ProtocolRequestType::PeripheralRequest.to_value() {
                // TODO: Decouple the buffer from being 'packet' typed and length.
                let mut raw_proto = self.packet_buf.raw_mut();
                let n = req.read(raw_proto).await?;
                self.process_peripheral_request_data(&self.packet_buf.raw()[0..n]).await;
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
                self.rtc.wait_ms(10).await;

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
                // TODO: Don't dequeue until the host has ACK'ed the response?
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

                // TODO: Re-use the packet buffer.
                let mut raw_proto = common::fixed::vec::FixedVec::<u8, 256>::new();

                let network_config = self.radio_socket.lock_network_config().await;
                if let Some(network_config) = network_config.get() {
                    if let Err(_) = network_config.serialize_to(&protobuf::SerializeOptions::default(), &mut raw_proto) {
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
                // TODO: Decouple the buffer from being 'packet' typed and length.
                let mut buffer = self.packet_buf.raw_mut();

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
            } else if setup.bRequest == ProtocolRequestType::PeripheralResponse.to_value() {
                // TODO: Decouple the buffer from being 'packet' typed and length.
                let mut buffer = self.packet_buf.raw_mut();
                
                if (setup.wLength as usize) < buffer.len() {
                    res.stale();
                    return Ok(());
                }

                // TODO: Don't consume unless the host ACKs the data?
                if let Some(controller) = self.peripherals_controller {
                    let n = controller.read_response(buffer).await;
                    res.write(&buffer[0..n]).await?;
                    return Ok(());
                }
            } else if setup.bRequest == ProtocolRequestType::GetClockTime.to_value() {
                if (setup.wLength as usize) < 4 {
                    res.stale();
                    return Ok(());
                }

                if let Some(controller) = self.peripherals_controller {
                    if let Some(t) = controller.get_clock_time().await {
                        let buffer = t.to_le_bytes();
                        res.write(&buffer[..]).await?;
                        return Ok(());
                    }
                }
            } else if setup.bRequest == ProtocolRequestType::GetIdleCounter.to_value() {
                if (setup.wLength as usize) < 4 {
                    res.stale();
                    return Ok(());
                }

                let num = crate::idle::idle_counter_value();
                let buffer = num.to_le_bytes();
                res.write(&buffer[..]).await?;
                return Ok(());
            }
        }

        USBDeviceDefaultHandler::new(self.descriptors)
            .handle_control_response(setup, res)
            .await
    }

    async fn handle_normal_response_impl<'a>(
        &'a mut self,
        endpoint_index: usize,
        mut res: USBDeviceNormalResponse<'a>,
    ) -> Result<(), USBError> {

        if let Some(controller) = self.peripherals_controller {
            // TODO: We can probably avoid copying and just re-use the same buffer as the controller.
            let mut buf = Aligned::<_, u32>::new([0u8; 64]);
            let n = controller.read_response(&mut buf[..]).await;
            let n_padded = fast_next_multiple_of_4(n);
            
            if n > 0 {
                res.write_aligned(&buf[0..n_padded]).await?;
            }
        }

        Ok(())
    }

}

#[inline(always)]
fn fast_next_multiple_of_4(n: usize) -> usize {
    // (n + alignment - 1) & !(alignment - 1)
    (n + 3) & !3
}

pub async fn protocol_usb_thread_fn<D: ProtocolUSBDescriptorSet>(
    descriptors: D,
    mut usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    peripherals_controller: Option<&'static PeripheralsController>,
    rtc: RTC,
) {
    usb.run(ProtocolUSBHandler::new(
        descriptors,
        radio_socket,
        peripherals_controller,
        rtc,
    ))
    .await;
}
