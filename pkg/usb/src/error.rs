
#[derive(Debug, PartialEq, Fail)]
pub enum Error {
    /// A transfer stopped because the user closed the device associated with it.
    DeviceClosing,

    /// The physical device was disconnected before or while making a transfer.
    DeviceDisconnected,

    /// The transfer failed for an unknown reason.
    /// NOTE: If the device was just disconnected, then this may get returned for a short period of
    /// time before DeviceDisconnected is returned on future transfers.
    TransferFailure,

    TransferStalled,

    TransferCancelled,

    EndpointNotFound,

    /// The user attempted to send write to an IN endpoint or read from an OUT endpoint.
    EndpointWrongDirection,

    Overflow,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "usb::Error::{:?}", self)
    }
}