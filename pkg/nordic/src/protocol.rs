use core::future::Future;

use base_util::aligned::Aligned;
use common::attribute::GetAttributeValue;
use common::errors::*;
use common::fixed::vec::FixedVec;
use common::list::Appendable;
use executor::channel::Channel;
use executor::futures::*;
use logging::Logger;
use nordic_proto::nordic::{NetworkConfig, SensorConfig};
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
use crate::sensor::config_store::SensorConfigStore;

pub trait ProtocolUSBDescriptorSet =
    usb::DescriptorSet + GetAttributeValue<usb::dfu::DFUInterfaceNumberTag> + Copy + 'static;

pub struct ProtocolUSBHandler<D> {
    inner: BasicUSBHandler<D>,
    // TODO: Make this mandatory now that this is separate from BasicUSBHandler.
    peripherals_controller: Option<&'static PeripheralsController>,
}

pub struct BasicUSBHandler<D> {
    descriptors: D,
    radio_socket: Option<&'static RadioSocket>,
    sensor_config_store: Option<&'static SensorConfigStore>,
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
        self.inner.handle_control_request_impl(setup, req)
    }

    fn handle_control_response<'a>(
        &'a mut self,
        setup: SetupPacket,
        res: USBDeviceControlResponse<'a>,
    ) -> Self::HandleControlResponseFuture<'a> {
        self.inner.handle_control_response_impl(setup, res)
    }

    fn handle_normal_request<'a>(
        &'a mut self,
        endpoint_index: usize,
        mut req: USBDeviceNormalRequest<'a>,
    ) -> Self::HandleNormalRequestFuture<'a> {
        async move {
            // TODO: Decouple the buffer from being 'packet' typed and length.
            let mut raw_proto = self.inner.packet_buf.raw_mut();
            let n = req.read_aligned(raw_proto)?;
            self.process_peripheral_request_data(&self.inner.packet_buf.raw()[0..n]);

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

impl<D: ProtocolUSBDescriptorSet> USBDeviceHandler for BasicUSBHandler<D> {
    type HandleResetFuture<'a> = impl Future<Output = ()> + 'a;

    type HandleControlRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type PollNormalResponseReadyFuture<'a> = impl Future<Output = ()> + 'a;

    fn handle_reset<'a>(
        &'a mut self,
    ) -> Self::HandleResetFuture<'a> {
        async move { () }
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
        async move { Ok(()) }
    }

    fn handle_normal_response<'a>(
        &'a mut self,
        endpoint_index: usize,
        res: USBDeviceNormalResponse<'a>,
    ) -> Self::HandleNormalResponseFuture<'a> {
        async move { Ok(()) }
    }

    fn poll_normal_response_ready<'a>(
        &'a self,
        endpoint_index: usize,
    ) -> Self::PollNormalResponseReadyFuture<'a> {
        async move {
            executor::futures::pending().await
        }
    }
}

impl<D: ProtocolUSBDescriptorSet> ProtocolUSBHandler<D> {
    pub fn new(
        descriptors: D,
        radio_socket: Option<&'static RadioSocket>,
        peripherals_controller: Option<&'static PeripheralsController>,
        rtc: RTC,
    ) -> Self {
        Self {
            inner: BasicUSBHandler::new(descriptors, radio_socket, None, rtc),
            peripherals_controller,
        }
    }

    fn process_peripheral_request_data(&self, data: &[u8]) {
        let n = data.len();
        let mut i = 0;
        // TODO: Send back errors whenever this notices an inconsistency of parse failure.
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
                    // log!("PARSE FAIL");
                    return;
                }
            };

            // TODO: If we get no sequence, imply that it is the same as the last received request + 1

            if let Some(controller) = &self.peripherals_controller {
                controller.execute(&proto);
            }
        }

    }


    async fn handle_normal_response_impl<'a>(
        &'a mut self,
        endpoint_index: usize,
        mut res: USBDeviceNormalResponse<'a>,
    ) -> Result<(), USBError> {

        if let Some(controller) = self.peripherals_controller {
            // TODO: We can probably avoid copying and just re-use the same buffer as the controller.
            let mut buf = Aligned::<_, u32>::new([0u8; 64]);
            let n = controller.read_response(&mut buf[..]);
            let n_padded = fast_next_multiple_of_4(n);
            
            if n > 0 {
                res.write_aligned(&buf[0..n_padded])?;
            }
        }

        Ok(())
    }

}

impl<D: ProtocolUSBDescriptorSet> BasicUSBHandler<D> {
    pub fn new(
        descriptors: D,
        radio_socket: Option<&'static RadioSocket>,
        sensor_config_store: Option<&'static SensorConfigStore>,
        rtc: RTC,
    ) -> Self {
        Self {
            descriptors,
            radio_socket,
            sensor_config_store,
            rtc,
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
            if setup.bRequest == ProtocolRequestType::SetNetworkConfig.to_value() {
                log!("USB SET CFG");

                if let Some(proto) = self.control_request_with_proto::<NetworkConfig>(req).await? {
                    // Ignore errors.
                    let _ = self.radio_socket.as_ref().unwrap().set_network_config(proto).await;
                    log!("=> DONE");
                }

                return Ok(());
            }

            if setup.bRequest == ProtocolRequestType::SetSensorConfig.to_value() {
                if let Some(proto) = self.control_request_with_proto::<SensorConfig>(req).await? {
                    let _ = self.sensor_config_store.as_ref().unwrap().set_config(proto).await;
                }

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

    async fn control_request_with_proto<'a, M: protobuf::StaticMessage>(
        &mut self,
        mut req: USBDeviceControlRequest<'a>,
    ) -> Result<Option<M>, USBError> {
        // TODO: Decouple the buffer from being 'packet' typed and length.
        let mut raw_proto = self.packet_buf.raw_mut();
        let n = req.read(&mut raw_proto).await?;

        if n == 0 {
            return Ok(None);
        }

        let len = raw_proto[0] as usize;
        if 1 + len > n {
            return Ok(None);
        }

        let proto = match M::parse(&raw_proto[1..(1 + len)]) {
            Ok(v) => v,
            Err(e) => {
                log!("PARSE FAIL");
                return Ok(None);
            }
        };

        Ok(Some(proto))
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b11000000
        /* Device-to-host | Vendor | Device */
        {
            if setup.bRequest == ProtocolRequestType::GetNetworkConfig.to_value() {
                log!("USB GETCFG");

                let radio_socket = self.radio_socket.as_ref().unwrap();

                let network_config = radio_socket.lock_network_config().await;

                if let Some(network_config) = network_config.get() {
                    Self::control_response_with_proto(network_config, res).await?;
                } else {
                    res.write(&[]).await;
                }

                drop(network_config);

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

                // Pad since sneding back an uneven number of bytes causing USBD bugs.
                while n < buffer.len() {
                    buffer[n] = 0;
                    n += 1;
                }

                res.write(&buffer[0..n]).await?;
                return Ok(());
            } else if setup.bRequest == ProtocolRequestType::GetSensorConfig.to_value() {
                let config = self.sensor_config_store.as_ref().unwrap().get_config().await.unwrap();
                return Self::control_response_with_proto(&config, res).await;
            }
        }

        USBDeviceDefaultHandler::new(self.descriptors)
            .handle_control_response(setup, res)
            .await
    }

    async fn control_response_with_proto<'a, M: protobuf::Message>(
        proto: &M,
        mut res: USBDeviceControlResponse<'a>
    ) ->  Result<(), USBError> {
        // TODO: Re-use the packet buffer.
        let mut raw_proto = common::fixed::vec::FixedVec::<u8, 256>::new();
        raw_proto.push(0);

        if let Err(_) = proto.serialize_to(&protobuf::SerializeOptions::default(), &mut raw_proto) {
            // TODO: Make sure this returns an error over USB?
            log!("USB SER FAIL");
            res.stale();
            return Ok(());
        }

        raw_proto[0] = (raw_proto.len() - 1) as u8;

        let n_padded = fast_next_multiple_of_4(raw_proto.len());
        while raw_proto.len() < n_padded {
            raw_proto.push(0);
        }

        res.write(raw_proto.as_ref()).await
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
    radio_socket: Option<&'static RadioSocket>,
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

pub async fn basic_usb_thread_fn<D: ProtocolUSBDescriptorSet>(
    descriptors: D,
    mut usb: USBDeviceController,
    radio_socket: Option<&'static RadioSocket>,
    rtc: RTC,
) {
    usb.run(BasicUSBHandler::new(
        descriptors,
        radio_socket,
        None,
        rtc,
    ))
    .await;
}

pub async fn sensor_usb_thread_fn<D: ProtocolUSBDescriptorSet>(
    descriptors: D,
    mut usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    sensor_config_store: &'static SensorConfigStore,
    rtc: RTC,
) {
    usb.run(BasicUSBHandler::new(
        descriptors,
        Some(radio_socket),
        Some(sensor_config_store),
        rtc,
    ))
    .await;
}

