use std::sync::Arc;

use common::async_std::channel;
use common::bytes::Bytes;
use common::errors::*;
use common::task::ChildTask;

use crate::channel::{Channel, MessageRequestBody};
use crate::client_types::{ClientRequestContext, ClientStreamingRequest, ClientStreamingResponse};
use crate::server::Http2RequestHandler;
use crate::server_types::{ServerRequestContext, ServerStreamRequest, ServerStreamResponse};
use crate::service::Service;

/// rpc::Channel implementation which directly wraps an rpc::Service for making
/// RPCs to an in-process service.
pub struct LocalChannel {
    handler: Arc<Http2RequestHandler>,
}

impl LocalChannel {
    pub fn new(service: Arc<dyn Service>) -> Self {
        Self {
            handler: Arc::new(Http2RequestHandler::new(service)),
        }
    }

    async fn request_handler(
        handler: Arc<Http2RequestHandler>,
        service_name: String,
        method_name: String,
        request_context: ClientRequestContext,
        request_receiver: channel::Receiver<Result<Option<Bytes>>>,
    ) -> Result<http::Response> {
        let server_request_context = ServerRequestContext {
            metadata: request_context.metadata,
        };
        let server_request_body = Box::new(MessageRequestBody::new(request_receiver));
        let server_request = ServerStreamRequest::new(server_request_body, server_request_context);

        Ok(handler
            .handle_parsed_request(&service_name, &method_name, server_request)
            .await)
    }
}

#[async_trait]
impl Channel for LocalChannel {
    async fn call_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
    ) -> (ClientStreamingRequest<()>, ClientStreamingResponse<()>) {
        let (req_sender, req_receive) = channel::unbounded();

        let client_req = ClientStreamingRequest::new(req_sender);
        let client_res = ClientStreamingResponse::from_response(Self::request_handler(
            self.handler.clone(),
            service_name.to_string(),
            method_name.to_string(),
            request_context.clone(),
            req_receive,
        ));

        (client_req, client_res)
    }
}