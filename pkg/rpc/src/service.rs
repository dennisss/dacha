use common::bytes::Bytes;
use common::errors::*;

use crate::server_types::*;


#[async_trait]
pub trait Service: Send + Sync {
    /// Name of the service. 
    fn service_name(&self) -> &'static str;

    /// Names of all methods which this service can accept. (used for reflection).
    fn method_names(&self) -> &'static [&'static str];
    
    async fn call<'a>(
        &self,
        method_name: &str,
        request: ServerStreamRequest<()>,
        response: ServerStreamResponse<'a, ()>
    ) -> Result<()>;
}