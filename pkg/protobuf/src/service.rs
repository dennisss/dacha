use common::bytes::Bytes;
use common::errors::*;

// TODO: See standard GRPC codes here:
// https://github.com/grpc/grpc/blob/master/doc/statuscodes.md

/// A request is simply a possibly un
#[async_trait]
pub trait Request: Send + Sync {
    /// None is returned when the end of the stream is hit. (or the second time
    /// during a non-streaming client request).
    async fn next(&mut self) -> Result<Option<Bytes>>;
}

#[async_trait]
pub trait Response: Send + Sync {
    async fn send(&mut self, message: Bytes) -> Result<()>;
}

// Request is a Box<Stream>

/// A named collection of methods which can be called on a server.
///
/// Typically a user will not directly implement this. Instead when a protobuf service named 'Name'
/// is compiled, we will autogenerate a 'NameService' trait which should be implemented by the
/// server and then '.into_service()' can be called on an instance of 'NameService' to create a
/// 'Service' instance.
#[async_trait]
pub trait Service: Send + Sync {
    /// Name of the service. 
    fn service_name(&self) -> &'static str;

    /// Names of all methods which this service can accept. (used for reflection).
    fn method_names(&self) -> &'static [&'static str];

    /// Executes a method on this service.
    ///
    /// Arguments:
    /// - method_name: Name of the method being requested.
    /// - request: Serialized form of the 
    ///
    /// TODO: Should return a GRPC compatible status.
    async fn call(&self, method_name: &str, request: Bytes) -> Result<Bytes>;
}

#[async_trait]
pub trait Channel {
    async fn call(
        &self,
        service_name: &'static str,
        method_name: &'static str,
        request_bytes: Bytes,
    ) -> Result<Bytes>;
}
