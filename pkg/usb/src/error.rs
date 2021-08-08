#[derive(Debug, Fail)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    ///
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

    Overflow,
}
