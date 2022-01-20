use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use common::async_std::channel;
use common::bytes::Buf;
use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use http::header::*;
use http::headers::content_type::MediaType;
use http::ClientInterface;

use crate::client_types::*;
use crate::media_type::*;
use crate::message::MessageSerializer;
use crate::metadata::*;
use crate::status::*;

/*
Scenarios:
- Request Stream, Response Stream
    - Return (ClientRequestSender, ClientResponseReceiver)
    - Challenge: Can we simultaneously read and write?
    - Technically once we have received the status trailers,

    -

- Request Unary, Response Stream
    - Return (ClientResponseReceiver)

- Big question:
    - How do we know from the sending end that the receiving end is done?
    - If we aren't activel reading things, then it is

- Request Stream, Response Unary
    - Challenge: Prevent accessing the response object until we are done sending everything
    - Challenge: want to see the head metadata right away
    - Return (ClientStreamRequest, )

- Challenge: If one half fails, then the other half will also start failing, so we ideally want
  to gate on the first real error.

- Unary, Unary
    - Easy.


Basically if either side is unary,


*/

#[async_trait]
pub trait Channel: 'static + Send + Sync {
    /// Sends a serialized stream of serialized messages to a remote service
    /// implementation. Returns a stream of received serialized messages
    /// and/or an error.
    async fn call_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        // request_stream: Box<dyn 'static + Streamable<Item=Result<Bytes>>>,
    ) -> (ClientStreamingRequest<()>, ClientStreamingResponse<()>);
}

impl dyn Channel {
    pub async fn call_stream_stream<Req: protobuf::Message, Res: protobuf::Message>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
    ) -> (ClientStreamingRequest<Req>, ClientStreamingResponse<Res>) {
        let (req, res) = self
            .call_raw(service_name, method_name, request_context)
            .await;
        (req.into(), res.into())
    }

    pub async fn call_unary_stream<Req: protobuf::Message, Res: protobuf::Message>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_value: &Req,
    ) -> ClientStreamingResponse<Res> {
        let (mut req, res) = self
            .call_stream_stream(service_name, method_name, request_context)
            .await;

        // NOTE: If the send failed, then the response should get an error.
        let _ = req.send(request_value).await;
        req.close().await;

        res
    }

    pub async fn call_unary_unary<Req: protobuf::Message, Res: protobuf::Message>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_value: &Req,
    ) -> ClientResponse<Res> {
        let mut response = self
            .call_unary_stream(service_name, method_name, request_context, request_value)
            .await;
        let response_value = self.call_unary_unary_impl(&mut response).await;

        ClientResponse {
            context: response.context,
            result: response_value,
        }
    }

    async fn call_unary_unary_impl<Res: protobuf::Message>(
        &self,
        response: &mut ClientStreamingResponse<Res>,
    ) -> Result<Res> {
        let response_message = response.recv().await;
        if response_message.is_some() && !response.recv().await.is_none() {
            return Err(crate::Status::unimplemented("Expected only one response message").into());
        }

        response.finish().await?;

        Ok(response_message
            .ok_or_else(|| crate::Status::unimplemented("Unary RPC returned OK without a body"))?)
    }

    pub async fn call_stream_unary<Req: protobuf::Message, Res: protobuf::Message>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
    ) -> ClientStreamingCall<Req, Res> {
        let (request, response) = self
            .call_stream_stream(service_name, method_name, request_context)
            .await;

        ClientStreamingCall::new(request, response)
    }
}

pub struct Http2Channel {
    client: http::Client,
}

impl Http2Channel {
    pub fn create(options: http::ClientOptions) -> Result<Self> {
        let client = http::Client::create(options.set_force_http2(true))?;

        Ok(Self { client })
    }

    async fn call_raw_impl(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_receiver: channel::Receiver<Result<Option<Bytes>>>,
    ) -> Result<ClientStreamingResponse<()>> {
        let body = Box::new(MessageRequestBody::new(request_receiver));

        // TODO: Use GET for methods known to be idempotnet.
        let mut request = http::RequestBuilder::new()
            .method(http::Method::POST)
            // TODO: Add the full package path.
            .path(format!("/{}/{}", service_name, method_name))
            // TODO: No gurantee that we were given proto data.
            .header(
                CONTENT_TYPE,
                RPCMediaType {
                    protocol: RPCMediaProtocol::Default,
                    serialization: RPCMediaSerialization::Proto,
                }
                .to_string(),
            )
            .body(body)
            .build()?;

        request_context
            .metadata
            .append_to_headers(&mut request.head.headers)?;

        let client = self.client.clone();

        // TODO: Need to implement custom logic for retrying service RPC errors (some
        // GRPC statuses should only be returned by the service exclusively).
        let response = async move { client.request(request).await };
        Ok(ClientStreamingResponse::from_response(response))
    }
}

#[async_trait]
impl Channel for Http2Channel {
    async fn call_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
    ) -> (ClientStreamingRequest<()>, ClientStreamingResponse<()>) {
        // TODO: Improve the tuning on this bound.
        let (request_sender, request_receiver) = channel::bounded(2);
        let request = ClientStreamingRequest::new(request_sender);

        let result = self
            .call_raw_impl(service_name, method_name, request_context, request_receiver)
            .await;

        let response = match result {
            Ok(res) => res,
            Err(e) => ClientStreamingResponse::from_error(e),
        };

        (request, response)
    }
}

/// http::Body for serializing client requests are separate message frames.
///
/// TODO: For the case of a unary RPC, this should be optimized to be retryable.
pub(crate) struct MessageRequestBody {
    request_receiver: channel::Receiver<Result<Option<Bytes>>>,
    remaining_bytes: Bytes,
}

impl MessageRequestBody {
    pub fn new(request_receiver: channel::Receiver<Result<Option<Bytes>>>) -> Self {
        Self {
            request_receiver,
            remaining_bytes: Bytes::new(),
        }
    }
}

#[async_trait]
impl http::Body for MessageRequestBody {
    fn len(&self) -> Option<usize> {
        None
    }
    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for MessageRequestBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            if !self.remaining_bytes.is_empty() {
                let n = std::cmp::min(self.remaining_bytes.len(), buf.len());
                buf[0..n].copy_from_slice(&self.remaining_bytes[0..n]);

                self.remaining_bytes.advance(n);

                // NOTE: We always stop after at least some amount of data is available to
                // ensure that readers are unblocked.
                return Ok(n);
            }

            let data = self.request_receiver.recv().await;
            match data {
                Ok(Ok(Some(data))) => {
                    self.remaining_bytes = Bytes::from(MessageSerializer::serialize(&data, false));
                }
                Ok(Ok(None)) => {
                    return Ok(0);
                }
                Ok(Err(e)) => {
                    // Custom failure reason (non-cancellation).
                    return Err(e);
                }
                Err(_) => {
                    // The sender was dropped before the None (end of stream indicator) was sent
                    // so we'll consider this to be an incomplete stream and inform the other side.
                    return Err(Status::cancelled("").into());
                }
            }
        }
    }
}
