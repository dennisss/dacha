use core::future::Future;

use executor::mutex::Mutex;
use nordic_proto::usb_descriptors::*;
use usb::descriptors::{SetupPacket, StandardRequestType};
use usb::hid::HIDDescriptorType;

use crate::keyboard::state::*;
use crate::protocol::ProtocolUSBHandler;
use crate::radio_socket::RadioSocket;
use crate::timer::Timer;
use crate::usb::controller::*;
use crate::usb::handler::{USBDeviceHandler, USBError};

pub struct KeyboardUSBHandler {
    state: &'static Mutex<KeyboardState>,
    inner_handler: ProtocolUSBHandler<KeyboardUSBDescriptors>,
}

impl USBDeviceHandler for KeyboardUSBHandler {
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

impl KeyboardUSBHandler {
    pub fn new(
        state: &'static Mutex<KeyboardState>,
        radio_socket: &'static RadioSocket,
        timer: Timer,
    ) -> Self {
        let inner_handler =
            ProtocolUSBHandler::new(KEYBOARD_USB_DESCRIPTORS, radio_socket, timer.clone());

        Self {
            state,
            inner_handler,
        }
    }

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b00100001 /* Host-to-device | Class | Interface */
            && setup.wIndex == get_attr!(&KEYBOARD_USB_DESCRIPTORS, usb::hid::HIDInterfaceNumberTag) as u16
        {
            /*
            Linux will configure a duration of 0.
            */

            if setup.bRequest == usb::hid::HIDRequestType::SET_IDLE.to_value() {
                let report_id = setup.wValue as u8;
                let duration = (setup.wValue >> 8) as u8;
                if report_id != 0 {
                    req.stale();
                    return Ok(());
                }

                {
                    let mut state = self.state.lock().await;
                    state.idle_timeout = (duration as usize) * 4;
                }

                req.read(&mut []).await;
                return Ok(());
            }

            if setup.bRequest == usb::hid::HIDRequestType::SET_REPORT.to_value() {
                let mut leds = [0];
                let n = req.read(&mut leds).await?;
                return Ok(());
            }

            if setup.bRequest == usb::hid::HIDRequestType::SET_PROTOCOL.to_value() {
                // TODO: If this occurs then we may want to clear our send buffer of any values
                // from the old protocol.
                {
                    let mut state = self.state.lock().await;
                    state.protocol = KeyboardUSBProtocol::from_value(setup.wValue as u8);
                }
                req.read(&mut []).await?;
                return Ok(());
            }
        }

        // wValue: ((duration as u16) << 8) | (report_id as u16),

        self.inner_handler.handle_control_request(setup, req).await
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b10100001 /* Device-to-Host | Class | Interface */
            && setup.wIndex
                == get_attr!(&KEYBOARD_USB_DESCRIPTORS, usb::hid::HIDInterfaceNumberTag) as u16
        {
            if setup.bRequest == usb::hid::HIDRequestType::GET_IDLE.to_value() {
                let idle_timeout = self.state.lock().await.idle_timeout;

                let mut data = [(idle_timeout / 4) as u8];
                res.write(&data).await?;
                return Ok(());
            }

            if setup.bRequest == usb::hid::HIDRequestType::GET_PROTOCOL.to_value() {
                let protocol = self.state.lock().await.protocol;
                let mut data = [protocol.to_value()];
                res.write(&data).await?;
                return Ok(());
            }
        }

        if setup.bmRequestType == 0b10000001 {
            if setup.bRequest == StandardRequestType::GET_DESCRIPTOR as u8 {
                let typ = (setup.wValue >> 8) as u8;
                if typ == HIDDescriptorType::Report.to_value() {
                    res.write(KEYBOARD_HID_REPORT_DESCRIPTOR).await?;
                    return Ok(());
                }
            }
        }

        self.inner_handler.handle_control_response(setup, res).await
    }
}
