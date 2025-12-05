use core::future::Future;

use common::errors::*;
use usb::descriptors::SetupPacket;

use crate::usb::controller::{
    USBDeviceControlRequest, USBDeviceControlResponse,
    USBDeviceNormalRequest,
    USBDeviceNormalResponse,
};

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
    type HandleResetFuture<'a>: Future<Output = ()> + 'a
    where
        Self: 'a;

    type HandleControlRequestFuture<'a>: Future<Output = Result<(), USBError>> + 'a
    where
        Self: 'a;

    type HandleControlResponseFuture<'a>: Future<Output = Result<(), USBError>> + 'a
    where
        Self: 'a;

    type HandleNormalRequestFuture<'a>: Future<Output = Result<(), USBError>> + 'a
    where
        Self: 'a;

    type HandleNormalResponseFuture<'a>: Future<Output = Result<(), USBError>> + 'a
    where
        Self: 'a;

    type PollNormalResponseReadyFuture<'a>: Future<Output = ()> + 'a
    where
        Self: 'a;

    // TODO: Implement this.
    fn handle_reset<'a>(
        &'a mut self,
    ) -> Self::HandleResetFuture<'a>;

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

    /// Called when a Bulk/Interrupt packet has been received from the host.
    ///
    /// The packet might have already been acknowledged but additional requests
    /// won't be accepted until the given one is read.
    fn handle_normal_request<'a>(
        &'a mut self,
        endpoint_index: usize,
        req: USBDeviceNormalRequest<'a>,
    ) -> Self::HandleNormalRequestFuture<'a>;

    fn handle_normal_response<'a>(
        &'a mut self,
        endpoint_index: usize,
        res: USBDeviceNormalResponse<'a>,
    ) -> Self::HandleNormalResponseFuture<'a>;

    fn poll_normal_response_ready<'a>(
        &'a self,
        endpoint_index: usize,
    ) -> Self::PollNormalResponseReadyFuture<'a>;
}
