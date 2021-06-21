use crate::metadata::*;
use crate::status::*;

pub struct ClientUnaryResponse<T> {
    /// If the RPC was successful with an OK status, then this will contain the value of the
    /// response message, otherwise this will contain a Status.
    ///
    /// NOTE: In some cases it is possible that we receive a response value and a non-OK status.
    /// In these cases we will ignore the value.
    pub result: StatusResult<T>,

    pub context: ClientResponseContext
}

pub struct ClientResponseContext {
    pub metadata: ResponseMetadata
}

pub struct ServerUnaryResponse<T: protobuf::Message> {
    pub result: StatusResult<T>,
    pub context: ServerResponseContext
}

impl<T: protobuf::Message> std::convert::From<StatusResult<T>> for ServerUnaryResponse<T> {
    fn from(result: StatusResult<T>) -> Self {
        Self { result, context: ServerResponseContext::default() }
    }
}

impl<T: protobuf::Message> std::convert::From<T> for ServerUnaryResponse<T> {
    fn from(value: T) -> Self {
        Self { result: Ok(value), context: ServerResponseContext::default() }
    }
}


#[derive(Default)]
pub struct ServerResponseContext {
    pub metadata: ResponseMetadata
}

/// 
#[derive(Default)]
pub struct ResponseMetadata {
    pub head_metadata: Metadata,
    pub trailer_metadata: Metadata
}