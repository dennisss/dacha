use std::collections::HashMap;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use http::header::*;
use http::status_code::*;

use crate::constants::GRPC_PROTO_TYPE;
use crate::request::*;
use crate::response::*;
use crate::message::{MessageReader, UnaryMessageBody};
use crate::metadata::*;
use crate::status::*;


#[async_trait]
pub trait Channel: Send + Sync {
    async fn call_unary_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_bytes: Bytes,
    ) -> Result<ClientUnaryResponse<Bytes>>;
}

impl dyn Channel {
    pub async fn call_unary<Req: protobuf::Message, Res: protobuf::Message>(
        &self,
        service_name: &str,
        method_name: &str,
        request: &ClientRequest<Req>
    ) -> Result<ClientUnaryResponse<Res>> {
        let request_bytes = request.value.serialize()?.into();

        let raw_response = self.call_unary_raw(
            service_name, method_name, &request.context, request_bytes).await?;

        let result = match raw_response.result {
            Ok(bytes) => Ok(Res::parse(bytes)?),
            Err(e) => Err(e)
        };

        Ok(ClientUnaryResponse {
            result,
            context: raw_response.context
        })
    }
}

pub struct Http2Channel {
    client: http::Client,
}

impl Http2Channel {
    pub fn create(options: http::ClientOptions) -> Result<Self> {
        Ok(Self {
            client: http::Client::create(options.set_force_http2(true))?,
        })
    }
}

#[async_trait]
impl Channel for Http2Channel {
    async fn call_unary_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_bytes: Bytes,
    ) -> Result<ClientUnaryResponse<Bytes>> {
        let mut request = http::RequestBuilder::new()
            .method(http::Method::POST)
            .path(format!("/{}/{}", service_name, method_name))
            // TODO: No gurantee that we were given proto data.
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE)
            .body(UnaryMessageBody::new(request_bytes))
            .build()?;

        request_context.metadata.append_to_headers(&mut request.head.headers)?;

        let mut response = self.client.request(request).await?;
        if response.head.status_code != OK {
            return Err(err_msg("Server responded with non-OK status"));
        }

        let response_type = response.head.headers.find_one(CONTENT_TYPE)?.value.to_ascii_str()?;
        if response_type != GRPC_PROTO_TYPE {
            return Err(format_err!("Received RPC response with unknown Content-Type: {}", response_type));
        }

        let mut reader = MessageReader::new(response.body.as_mut());
        
        let response_bytes = reader.read().await?;
        if response_bytes.is_some() && !reader.read().await?.is_none() {
            return Err(err_msg("Expected only one response message"));
        }

        let trailers = response.body.trailers().await?
            .ok_or_else(|| err_msg("Server responded without trailers"))?;

        let response_context = ClientResponseContext {
            metadata: ResponseMetadata {
                head_metadata: Metadata::from_headers(&response.head.headers)?,
                trailer_metadata: Metadata::from_headers(&trailers)?
            }
        };

        let status = Status::from_headers(&trailers)?;
        
        let result = {
            if status.is_ok() {
                Ok(response_bytes.ok_or_else(|| err_msg("RPC returned OK without a body"))?)
            } else {
                Err(status)
            }
        };

        Ok(ClientUnaryResponse { context: response_context, result })
    }
}
