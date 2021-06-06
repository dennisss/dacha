use std::collections::HashMap;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use http::header::*;
use http::status_code::*;

use crate::constants::GRPC_PROTO_TYPE;

pub struct Channel {
    client: http::Client,
}

impl Channel {
    pub fn create(uri: &str) -> Result<Self> {
        Ok(Self {
            client: http::Client::create(uri)?,
        })
    }
}

#[async_trait]
impl protobuf::service::Channel for Channel {
    async fn call(
        &self,
        service_name: &'static str,
        method_name: &'static str,
        request_bytes: Bytes,
    ) -> Result<Bytes> {
        let request = http::RequestBuilder::new()
            .method(http::Method::POST)
            .path(format!("/{}/{}", service_name, method_name))
            // TODO: No gurantee that we were given proto data.
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE)
            .body(http::BodyFromData(request_bytes))
            .build()?;

        let mut response = self.client.request(request).await?;
        if response.head.status_code != OK {
            return Err(err_msg("Request failed"));
        }

        // TODO: Check Content-Type?

        let mut response_bytes = vec![];
        response.body.read_to_end(&mut response_bytes).await?;

        Ok(response_bytes.into())
    }
}
