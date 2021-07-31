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

    TransferCancelled,

    EndpointNotFound,

    Overflow,
}
