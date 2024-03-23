use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::channel::spsc;
use executor::child_task::ChildTask;

use crate::channel::Channel;
use crate::client_types::{ClientRequestContext, ClientStreamingRequest, ClientStreamingResponse};
use crate::media_type::{RPCMediaProtocol, RPCMediaSerialization, RPCMediaType};
use crate::message_request_body::{MessageRequestBody, MessageRequestBuffer};
use crate::server::Http2RequestHandler;
use crate::server_types::{ServerRequestContext, ServerStreamRequest, ServerStreamResponse};
use crate::service::Service;
use crate::Http2Channel;

/// rpc::Channel implementation which directly wraps an rpc::Service for making
/// RPCs to an in-process service.
pub struct LocalChannel {
    handler: Arc<Http2RequestHandler>,
}

impl LocalChannel {
    pub fn new(service: Arc<dyn Service>) -> Self {
        Self {
            handler: Arc::new(Http2RequestHandler::new(service, false)),
        }
    }

    async fn request_handler(
        handler: Arc<Http2RequestHandler>,
        service_name: String,
        method_name: String,
        request_context: ClientRequestContext,
        request_receiver: spsc::Receiver<Result<Option<Bytes>>>,
        attempt_alive: spsc::Receiver<()>,
    ) -> http::Response {
        let server_request_context = ServerRequestContext {
            metadata: request_context.metadata,
        };

        let server_request_buffer = Arc::new(MessageRequestBuffer::new(0, request_receiver));

        let server_request_body = Box::new(MessageRequestBody::new(
            server_request_buffer,
            attempt_alive,
        ));
        let server_request = ServerStreamRequest::new(
            server_request_body,
            RPCMediaType {
                protocol: RPCMediaProtocol::Default,
                serialization: RPCMediaSerialization::Proto,
            },
            handler.codec_options.clone(),
            server_request_context,
        );

        handler
            .handle_parsed_request(
                &service_name,
                &method_name,
                server_request,
                RPCMediaType {
                    protocol: RPCMediaProtocol::Default,
                    serialization: RPCMediaSerialization::Proto,
                },
            )
            .await
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
        let (req_sender, req_receive) = spsc::bounded(2);

        let client_req = ClientStreamingRequest::new(req_sender);

        let handler = self.handler.clone();
        let request_context = request_context.clone();
        let service_name = service_name.to_string();
        let method_name = method_name.to_string();

        let client_res = ClientStreamingResponse::from_future_response(async move {
            let (sender, receiver) = spsc::bounded(0);

            let result = Self::request_handler(
                handler,
                service_name,
                method_name,
                request_context.clone(),
                req_receive,
                receiver,
            )
            .await;

            Http2Channel::process_existing_response(Ok(result), sender, &request_context).await
        });

        (client_req, client_res)
    }
}
