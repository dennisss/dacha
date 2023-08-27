use common::errors::*;

use crate::client_types::ClientRequestContext;

#[async_trait]
pub trait ChannelCredentialsProvider: 'static + Send + Sync {
    async fn attach_request_credentials(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &mut ClientRequestContext,
    ) -> Result<()>;
}
