use core::future::Future;

use usb::descriptors::SetupPacket;

use crate::usb::controller::{USBDeviceControlRequest, USBDeviceControlResponse};

pub trait USBDeviceHandler {
    type HandleControlRequestFuture<'a>: Future<Output = ()> + 'a
    where
        Self: 'a;

    type HandleControlResponseFuture<'a>: Future<Output = ()> + 'a
    where
        Self: 'a;

    fn handle_control_request<'a>(
        &'a mut self,
        setup: SetupPacket,
        req: USBDeviceControlRequest<'a>,
    ) -> Self::HandleControlRequestFuture<'a>;

    fn handle_control_response<'a>(
        &'a mut self,
        setup: SetupPacket,
        res: USBDeviceControlResponse<'a>,
    ) -> Self::HandleControlResponseFuture<'a>;
}
