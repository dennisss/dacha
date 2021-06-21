use crate::metadata::*;


pub struct ClientRequest<T> {
    pub value: T,
    pub context: ClientRequestContext
}

impl<T: protobuf::Message> std::convert::From<T> for ClientRequest<T> {
    fn from(value: T) -> Self {
        Self { value, context: ClientRequestContext::default() }
    }
}


/// Used by an RPC client to specify how a single RPC should be sent and what metadata
/// should be sent along with the RPC. 
#[derive(Default)]
pub struct ClientRequestContext {
    pub metadata: Metadata,
    pub idempotent: bool,
    pub fail_fast: bool
    // TODO: Deadline
}


pub struct ServerRequest<T: protobuf::Message> {
    pub value: T,
    pub context: ServerRequestContext
}

impl<T: protobuf::Message> std::ops::Deref for ServerRequest<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}


/// Server-side view of information related to this request.
pub struct ServerRequestContext {
    pub metadata: Metadata

    // metadata

    // connection information

    // deadline (if any)
}