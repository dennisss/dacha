use std::marker::PhantomData;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use common::task::ChildTask;
use http::header::CONTENT_TYPE;

use crate::media_type::RPCMediaProtocol;
use crate::media_type::RPCMediaSerialization;
use crate::media_type::RPCMediaType;
use crate::message::*;
use crate::metadata::*;
use crate::status::*;

/*
- Unary : Unary
    - Just return a ClientResponse

- Unary : Streaming
    - Just return a ClientStreamingResponse
    - API:
        - recv()
        - finish() -> Result<()>

- Streaming : Unary
    - Just return a ClientStreamingCall<Req, Res>
    - API:
        - send()
        - finish() -> Result<Res>

- Streaming : Streaming
    - Return a (ClientStreamingRequest, ClientStreamingResponse??)



ClientStreamingCall



(ClientStreamingWriter, ClientStreamResponse)
^ These must be tied together such that when writer fails,


ClientUnaryResponse

(ClientStreamingRequest, ClientPendingResponse

(ClientWriter, ClientReader)



ClientStreaming


*/

/// Used by an RPC client to specify how a single RPC should be sent and what
/// metadata should be sent along with the RPC.
#[derive(Default, Clone)]
pub struct ClientRequestContext {
    pub metadata: Metadata,
    pub idempotent: bool,
    pub wait_for_ready: bool, // TODO: Deadline
}

#[derive(Default)]
pub struct ClientResponseContext {
    /// Metadata received from the server.
    ///
    /// NOTE: This may not be complete if we received an invalid response from
    /// the server. Implementations should be resilient to this missing some
    /// keys.
    pub metadata: ResponseMetadata,
}

/// Response returned from an RPC with unary request and unary resposne.
pub struct ClientResponse<T> {
    pub context: ClientResponseContext,

    /// If the RPC was successful with an OK status, then this will contain the
    /// value of the response message, otherwise this will contain an error.
    /// In the latter case, this will usually be an rpc::Status if we
    /// successfully connected to the remote server.
    ///
    /// NOTE: In some cases it is possible that we receive a response value and
    /// a non-OK status. In these cases we will ignore the value.
    pub result: Result<T>,
}

/// Interface for sending a stream of messages to the server.
///
/// NOTE: Droping the ClientStreamingRequest before close() is executed will
/// mark the request stream is incomplete.
///
/// TODO: Double check that any failure in the ClientStreamingRequest is
/// propagated to the ClientStreamingResponse and vice versa.
pub struct ClientStreamingRequest<T> {
    sender: Option<channel::Sender<Result<Option<Bytes>>>>,
    phantom_t: PhantomData<T>,
}

impl ClientStreamingRequest<()> {
    pub(crate) fn new(sender: channel::Sender<Result<Option<Bytes>>>) -> Self {
        Self {
            sender: Some(sender),
            phantom_t: PhantomData,
        }
    }

    pub fn into<T: protobuf::Message>(self) -> ClientStreamingRequest<T> {
        ClientStreamingRequest {
            sender: self.sender,
            phantom_t: PhantomData,
        }
    }

    /// Creates a new request object which is permanently closed and no requests
    /// can be send using it.
    pub fn closed() -> Self {
        Self {
            sender: None,
            phantom_t: PhantomData,
        }
    }
}

impl<T> ClientStreamingRequest<T> {
    #[must_use]
    pub async fn send_bytes(&mut self, data: Bytes) -> bool {
        let sender = match self.sender.as_ref() {
            Some(v) => v,
            None => {
                return false;
            }
        };

        sender.send(Ok(Some(data))).await.is_ok()
    }

    /// Call after sending all messages to the server to indicate that no more
    /// messages will be sent for the current RPC.
    pub async fn close(&mut self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Ok(None)).await;
        }
    }
}

impl<T: protobuf::Message> ClientStreamingRequest<T> {
    /// Returns whether or not the message was sent. If not, then the connection
    /// was broken and the client should check the finish() method on the
    /// other end.
    #[must_use]
    pub async fn send(&mut self, message: &T) -> bool {
        let sender = match self.sender.as_ref() {
            Some(v) => v,
            None => {
                return false;
            }
        };

        // TODO: Verify that we see this error propagated to the response side.
        let data = match message.serialize() {
            Ok(v) => Ok(Some(v.into())),
            Err(e) => Err(e),
        };

        sender.send(data).await.is_ok()
    }
}

/// Response returned by an RPC with a unary request and streaming response.
///
/// TODO: Check that our HTTP2 implementation will close the entire stream once
/// the Server closes its stream (as there isn't any good usecase for continuing
/// to send client bytes in this case).
pub struct ClientStreamingResponse<Res> {
    pub context: ClientResponseContext,

    state: Option<ClientStreamingResponseState>,

    interceptor: Option<Arc<dyn ClientResponseInterceptor>>,

    phantom_t: PhantomData<Res>,
}

enum ClientStreamingResponseState {
    /// We are still waiting for an initial response / head metadata from the
    /// server.
    Head(ChildTask<Result<http::Response>>),

    /// We have gotten initial metadata and are receiving zero or more messages.
    Body(Box<dyn http::Body>),

    /// All messages have been received and we are
    Trailers(Box<dyn http::Body>),
    Error(Error),
}

impl<Res> ClientStreamingResponse<Res> {
    pub(crate) fn from_response<
        F: 'static + Send + std::future::Future<Output = Result<http::Response>>,
    >(
        response: F,
    ) -> Self {
        Self {
            context: ClientResponseContext {
                metadata: ResponseMetadata {
                    head_metadata: Metadata::default(),
                    trailer_metadata: Metadata::default(),
                },
            },
            state: Some(ClientStreamingResponseState::Head(ChildTask::spawn(
                response,
            ))),
            interceptor: None,
            phantom_t: PhantomData,
        }
    }

    pub fn from_error(error: Error) -> Self {
        Self {
            context: ClientResponseContext::default(),
            state: Some(ClientStreamingResponseState::Error(error)),
            interceptor: None,
            phantom_t: PhantomData,
        }
    }

    /// TODO: Support multiple hooks.
    pub fn set_interceptor(&mut self, interceptor: Arc<dyn ClientResponseInterceptor>) {
        self.interceptor = Some(interceptor);
    }
}

impl ClientStreamingResponse<()> {
    pub(crate) fn into<Res: protobuf::Message>(self) -> ClientStreamingResponse<Res> {
        ClientStreamingResponse {
            context: self.context,
            state: self.state,
            interceptor: self.interceptor,
            phantom_t: PhantomData,
        }
    }
}

impl<Res: protobuf::Message> ClientStreamingResponse<Res> {
    pub async fn recv(&mut self) -> Option<Res> {
        let data = match self.recv_bytes().await {
            Some(data) => data,
            None => return None,
        };

        match Res::parse(&data) {
            Ok(v) => Some(v),
            Err(e) => {
                self.state = Some(ClientStreamingResponseState::Error(e.into()));
                None
            }
        }
    }
}

impl<T> ClientStreamingResponse<T> {
    // TODO: Consider adding a method to wait for initial metadata?

    pub async fn recv_head(&mut self) {
        match self.state.take() {
            Some(ClientStreamingResponseState::Head(response)) => {
                if let Err(e) = self.recv_head_impl(response).await {
                    self.state = Some(ClientStreamingResponseState::Error(e));
                }
            }
            state @ _ => {
                self.state = state;
            }
        }
    }

    // TODO: One issue with this implementation is that if we ever drop the future,
    // the response will be in an invalid state?.
    //
    // TODO: Eventually make this pub(crate) again.
    pub async fn recv_bytes(&mut self) -> Option<Bytes> {
        loop {
            return match self.state.take() {
                Some(ClientStreamingResponseState::Head(response)) => {
                    if let Err(e) = self.recv_head_impl(response).await {
                        self.state = Some(ClientStreamingResponseState::Error(e));
                        None
                    } else {
                        // Loop again to wait on the first response from the body.
                        continue;
                    }
                }
                Some(ClientStreamingResponseState::Body(body)) => {
                    match self.recv_body(body).await {
                        Ok(value) => value,
                        Err(e) => {
                            self.state = Some(ClientStreamingResponseState::Error(e));
                            None
                        }
                    }
                }

                Some(state) => {
                    self.state = Some(state);
                    None
                }
                None => None,
            };
        }
    }

    async fn recv_head_impl(&mut self, response: ChildTask<Result<http::Response>>) -> Result<()> {
        let response = response.join().await?;

        if response.head.status_code != http::status_code::OK {
            return Err(crate::Status::unknown("Server responded with non-OK status").into());
        }

        let response_type = RPCMediaType::parse(&response.head.headers)
            .ok_or_else(|| err_msg("Response received without valid content type"))?;
        if response_type.protocol != RPCMediaProtocol::Default
            || response_type.serialization != RPCMediaSerialization::Proto
        {
            return Err(err_msg("Received unsupported media type"));
        }

        self.context.metadata.head_metadata = Metadata::from_headers(&response.head.headers)?;

        if let Some(interceptor) = &self.interceptor {
            interceptor
                .on_response_head(&mut self.context.metadata.head_metadata)
                .await?;
        }

        self.state = Some(ClientStreamingResponseState::Body(response.body));

        Ok(())
    }

    async fn recv_body(&mut self, mut body: Box<dyn http::Body>) -> Result<Option<Bytes>> {
        let mut reader = MessageReader::new(body.as_mut());

        let message_bytes = reader.read().await?;

        if let Some(message) = message_bytes {
            if message.is_trailers {
                return Err(err_msg("Did not expect a trailers message"));
            }

            // Keep trying to read more messages.
            self.state = Some(ClientStreamingResponseState::Body(body));
            Ok(Some(message.data))
        } else {
            self.state = Some(ClientStreamingResponseState::Trailers(body));
            Ok(None)
        }
    }

    /// Call after the response is fully read to receive the trailer metadata
    /// and RPC status.
    pub async fn finish(&mut self) -> Result<()> {
        let state = self
            .state
            .take()
            .ok_or_else(|| err_msg("Response in invalid state"))?;

        match state {
            ClientStreamingResponseState::Head(_) | ClientStreamingResponseState::Body(_) => {
                Err(err_msg("Response body hasn't been fully read yet"))
            }
            ClientStreamingResponseState::Error(e) => Err(e),
            ClientStreamingResponseState::Trailers(mut body) => {
                let trailers = body
                    .trailers()
                    .await?
                    .ok_or_else(|| err_msg("Server responded without trailers"))?;

                self.context.metadata.trailer_metadata = Metadata::from_headers(&trailers)?;

                let status = Status::from_headers(&trailers)?;

                if status.is_ok() {
                    Ok(())
                } else {
                    Err(status.into())
                }
            }
        }
    }
}

/// Interface for sending an RPC with a streaming request and returning a unary
/// response.
pub struct ClientStreamingCall<Req, Res> {
    request: ClientStreamingRequest<Req>,
    response: ClientStreamingResponse<Res>,
}

impl<Req: protobuf::Message, Res: protobuf::Message> ClientStreamingCall<Req, Res> {
    pub fn new(
        request: ClientStreamingRequest<Req>,
        response: ClientStreamingResponse<Res>,
    ) -> Self {
        Self { request, response }
    }

    pub fn context(&self) -> &ClientResponseContext {
        &self.response.context
    }

    #[must_use]
    pub async fn send(&mut self, message: &Req) -> bool {
        self.request.send(message).await
    }

    pub async fn finish(&mut self) -> Result<Res> {
        self.request.close().await;

        let response = self.response.recv().await;
        if response.is_some() && !self.response.recv().await.is_none() {
            return Err(err_msg("Expected only one response message"));
        }

        self.response.finish().await?;

        // If we are here, then we should have the message and all the metadata.

        Ok(response.ok_or_else(|| err_msg("Unary RPC returned OK without a body"))?)
    }
}

#[async_trait]
pub trait ClientResponseInterceptor: 'static + Send + Sync {
    async fn on_response_head(&self, metadata: &mut Metadata) -> Result<()>;
}
