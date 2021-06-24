use common::errors::*;

use crate::metadata::*;

pub struct ClientResponse<T> {
    /// If the RPC was successful with an OK status, then this will contain the value of the
    /// response message, otherwise this will contain an error. In the latter case, this will
    /// usually be an rpc::Status if we successfully connected to the remote server.
    ///
    /// NOTE: In some cases it is possible that we receive a response value and a non-OK status.
    /// In these cases we will ignore the value.
    pub result: Result<T>,

    pub context: ClientResponseContext
}

#[derive(Default)]
pub struct ClientResponseContext {
    /// Metadata received from the server.
    ///
    /// NOTE: This may not be complete if we received an invalid response from the server.
    /// Implementations should be resilient to this missing some keys.
    pub metadata: ResponseMetadata
}

pub struct ServerResponse<'a, T: protobuf::Message> {
    /// Value to be returned to the client. Only fully returned if the response 
    pub value: T,
    pub context: &'a mut ServerResponseContext
}

impl<'a, T: protobuf::Message> std::ops::Deref for ServerResponse<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T: protobuf::Message> std::ops::DerefMut for ServerResponse<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

// impl<T: protobuf::Message> std::convert::From<StatusResult<T>> for ServerResponse<T> {
//     fn from(result: StatusResult<T>) -> Self {
//         Self { result, context: ServerResponseContext::default() }
//     }
// }

// impl<T: protobuf::Message> std::convert::From<T> for ServerResponse<T> {
//     fn from(value: T) -> Self {
//         Self { value, context: ServerResponseContext::default() }
//     }
// }


#[derive(Default)]
pub struct ServerResponseContext {
    /// NOTE: We will still try to send any response metadata back to the client even if the RPC
    /// handler failed.
    pub metadata: ResponseMetadata
}

/// 
#[derive(Default)]
pub struct ResponseMetadata {
    pub head_metadata: Metadata,
    pub trailer_metadata: Metadata
}