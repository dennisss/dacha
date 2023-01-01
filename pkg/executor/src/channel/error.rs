/// NOTE: This is not an Error as we we'd prefer for T to be dropped ASAP rather
/// than being propagated through errors.
pub struct SendRejected<T> {
    pub value: T,
    pub error: SendError,
}

impl<T> From<SendRejected<T>> for common::errors::Error {
    fn from(value: SendRejected<T>) -> common::errors::Error {
        value.error.into()
    }
}

#[error]
#[derive(PartialEq)]
pub enum SendError {
    ReceiverDropped,

    /// The channel is corked (or we called try_send) and we are also at the max
    /// capacity so messages can't be added or removed.
    OutOfSpace,
}

#[error]
pub enum RecvError {
    SenderDropped,
}
