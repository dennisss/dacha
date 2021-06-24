use common::bytes::Bytes;
use common::errors::*;

use crate::request::*;
use crate::response::*;

#[async_trait]
pub trait Service: Send + Sync {
    /// Name of the service. 
    fn service_name(&self) -> &'static str;

    /// Names of all methods which this service can accept. (used for reflection).
    fn method_names(&self) -> &'static [&'static str];
    
    async fn call(
        &self,
        method_name: &str,
        request_context: ServerRequestContext,
        request_bytes: Bytes,
        response_context: &mut ServerResponseContext
    ) -> Result<Bytes>;
}