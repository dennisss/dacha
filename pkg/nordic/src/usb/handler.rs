use core::future::Future;

use common::errors::*;
use usb::descriptors::SetupPacket;

use crate::usb::controller::{USBDeviceControlRequest, USBDeviceControlResponse};

// TODO: Rename to USBDeviceError.
#[derive(PartialEq)]
pub enum USBError {
    Reset,
    Disconnected,
    /// A new setup packet has been received by the device while a previous
    /// SETUP packet was still being processed.
    NewSetupPacket,
}

pub trait USBDeviceHandler {
    type HandleControlRequestFuture<'a>: Future<Output = Result<(), USBError>> + 'a
    where
        Self: 'a;

    type HandleControlResponseFuture<'a>: Future<Output = Result<(), USBError>> + 'a
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
