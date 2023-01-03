//! Types used for fulfilling client-side RPCs.
//!
//! For Client Unary -> Server Unary RPCs:
//!
//! - A `ClientResponse` is returned to the caller.
//! - This object directly contains the one message or status.
//!
//! For Client Unary -> Server Streaming RPCs:
//!
//! - A `ClientStreamingResponse` is returned to the caller.
//! - The caller must:
//!   - Optionally call `ClientStreamingResponse::recv_head()` to read the head
//!     metadata.
//!   - Continously call `ClientStreamingResponse::recv()` to get messages until
//!     None is returned.
//!   - Call `ClientStreamingResponse::finish()` to get the status of the
//!     response.
//!
//! For Client Streaming -> Server Unary RPCs:
//!
//! - A `ClientStreamingCall` is returned to the caller.
//! - The caller must:
//!     - Call 'ClientStreamingCall::send()' with new messages until done or it
//!       returns false.
//!     - Call `ClientStreamingCall::finish()` to get the server response or
//!       status.
//!
//! For Client Streaming -> Server Streaming RPCs (BIDI):
//!
//! - A tuple of `(ClientStreamingRequest, ClientStreamingResponse)` are
//!   returned to the caller.
//! - The caller must:
//!     - Call `ClientStreamingRequest::send()` to send messages
//!     - Call `ClientStreamingRequest::close()` once all messages have been
//!       sent.
//!     - Use `ClientStreamingResponse` similarly to past cases for getting the
//!       server response.
//! - Internally all the other cases are implemented on top of these BIDI types.

use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use executor::channel::spsc;
use executor::child_task::ChildTask;
use executor::sync::Mutex;

use crate::media_type::RPCMediaProtocol;
use crate::media_type::RPCMediaSerialization;
use crate::media_type::RPCMediaType;
use crate::message::*;
use crate::message_request_body::MessageRequestBuffer;
use crate::metadata::*;
use crate::status::*;

/// Used by an RPC client to specify how a single RPC should be sent and what
/// metadata should be sent along with the RPC.
#[derive(Default, Clone)]
pub struct ClientRequestContext {
    /// Arbitrary key-value metadata to send to the server.
    pub metadata: Metadata,

    pub idempotent: bool,
    pub wait_for_ready: bool, // TODO: Deadline

    /// If true, we will read and buffer the entire before any part of it is
    /// returned to the RPC caller.
    ///
    /// - This will be internally set to true for unary responses.
    /// - This is required to support full retrying of RPCs.
    pub buffer_full_response: bool,
}

#[derive(Default, Clone)]
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
    sender: Option<spsc::Sender<Result<Option<Bytes>>>>,
    phantom_t: PhantomData<T>,
}

impl ClientStreamingRequest<()> {
    pub(crate) fn new(sender: spsc::Sender<Result<Option<Bytes>>>) -> Self {
        Self {
            sender: Some(sender),
            phantom_t: PhantomData,
        }
    }

    pub fn into<T: protobuf::StaticMessage>(self) -> ClientStreamingRequest<T> {
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
    pub fn cork(&mut self) {
        if let Some(sender) = &mut self.sender {
            sender.cork();
        }
    }

    #[must_use]
    pub async fn send_bytes(&mut self, data: Bytes) -> bool {
        let sender = match self.sender.as_mut() {
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
        if let Some(mut sender) = self.sender.take() {
            let _ = sender.send(Ok(None)).await;
            // sender will be implicitly uncorked on drop here.
        }
    }
}

impl<T: protobuf::StaticMessage> ClientStreamingRequest<T> {
    /// Returns whether or not the message was sent. If not, then the connection
    /// was broken and the client should check the finish() method on the
    /// other end.
    ///
    /// NOTE: A return value of true is no gurantee that the server actually
    /// processed the message.
    #[must_use]
    pub async fn send(&mut self, message: &T) -> bool {
        let sender = match self.sender.as_mut() {
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

#[async_trait]
pub trait ClientStreamingResponseInterface: 'static + Send {
    async fn recv_bytes(&mut self) -> Option<Bytes>;

    async fn finish(&mut self) -> Result<()>;

    fn context(&self) -> &ClientResponseContext;
}

/// Response returned by an RPC with zero or more messages.
pub struct ClientStreamingResponse<Res> {
    /// All the metadata we have currently received from the server.
    ///
    /// TODO: Eventually make this private.
    pub(crate) context: ClientResponseContext,

    state: Option<ClientStreamingResponseState>,

    interceptor: Option<Arc<dyn ClientResponseInterceptor>>,

    phantom_t: PhantomData<Res>,
}

enum ClientStreamingResponseState {
    /// The request is currently being sent through the channel and we are
    /// waiting for it to become available.
    Sending(ChildTask<Box<dyn ClientStreamingResponseInterface>>),

    /// We have received some response from the channel that can now be
    /// forwarded to the caller.
    Received(Box<dyn ClientStreamingResponseInterface>),

    /// We will return an error the next time the response is polled.
    Error(Error),
}

impl<Res> ClientStreamingResponse<Res> {
    pub(crate) fn from_future_response<
        F: 'static + Send + Future<Output = Box<dyn ClientStreamingResponseInterface>>,
    >(
        response: F,
    ) -> Self {
        Self {
            context: ClientResponseContext::default(),
            state: Some(ClientStreamingResponseState::Sending(ChildTask::spawn(
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

    /// Gets metadata associated with the response.
    ///
    /// Note that the returned object will be empty until recv_head() or recv()
    /// returns at least once (indicating that we het the HTTP response
    /// headers).
    ///
    /// TODO: This is now wrong.
    pub fn context(&self) -> &ClientResponseContext {
        &self.context
    }
}

impl ClientStreamingResponse<()> {
    pub(crate) fn into<Res: protobuf::StaticMessage>(self) -> ClientStreamingResponse<Res> {
        ClientStreamingResponse {
            context: self.context,
            state: self.state,
            interceptor: self.interceptor,
            phantom_t: PhantomData,
        }
    }
}

impl<Res: protobuf::StaticMessage> ClientStreamingResponse<Res> {
    pub async fn recv(&mut self) -> Option<Res> {
        let data = match self.recv_bytes().await {
            Some(data) => data,
            None => return None,
        };

        // NOTE: This parsing won't get retried by channels.
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
    /// Waits until we have received at least the head of the response.
    ///
    /// Once this completes, the response should have head metadata (or an
    /// error).
    ///
    /// TODO: How can a client tell that there is an error?
    pub async fn recv_head(&mut self) {
        match self.state.take() {
            Some(ClientStreamingResponseState::Sending(response)) => {
                self.recv_head_impl(response).await;
            }
            state @ _ => {
                self.state = state;
            }
        }
    }

    async fn recv_head_impl(
        &mut self,
        response: ChildTask<Box<dyn ClientStreamingResponseInterface>>,
    ) {
        // TODO: Standardize the error codes we will us for this.
        // These should also probably not be 'local' rpc statuses.
        // ^ Yes, Yeah. Let's do this in the HTTP2 channel code.

        let response = response.join().await;

        // TODO: Find a better solution than this.
        self.context = response.context().clone();

        self.state = Some(ClientStreamingResponseState::Received(response));

        if let Some(interceptor) = &self.interceptor {
            if let Err(e) = interceptor
                .on_response_head(&mut self.context.metadata.head_metadata)
                .await
            {
                self.state = Some(ClientStreamingResponseState::Error(e));
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
                Some(ClientStreamingResponseState::Sending(response)) => {
                    self.recv_head_impl(response).await;

                    // Loop again to wait on the first response from the body.
                    continue;
                }
                Some(ClientStreamingResponseState::Received(mut response)) => {
                    let data = response.recv_bytes().await;
                    self.state = Some(ClientStreamingResponseState::Received(response));
                    data
                }
                state @ Some(ClientStreamingResponseState::Error(_)) => {
                    self.state = state;
                    None
                }
                None => None,
            };
        }
    }

    /// Call after the response is fully read to receive the trailer metadata
    /// and RPC status.
    ///
    /// NOTE: It is invalid to call this twice. This does not take 'self'
    /// ownership to allow users to still access the metadata (especially the
    /// trailer metadata which may) after this returns.
    pub async fn finish(&mut self) -> Result<()> {
        let state = self
            .state
            .take()
            .ok_or_else(|| err_msg("Response in invalid state 2"))?;

        match state {
            ClientStreamingResponseState::Sending(_) => {
                Err(err_msg("Response hasn't been received yet"))
            }
            ClientStreamingResponseState::Received(mut response) => {
                let result = response.finish().await;

                // TODO: Find a better solution to this. We want to ensure that the context is
                // still available even after
                //
                // TODO: This will not reflect any edits made by request interceptors.
                self.context = response.context().clone();

                result
            }
            ClientStreamingResponseState::Error(e) => Err(e),
        }
    }
}

/// Interface for sending an RPC with a streaming request and returning a unary
/// response.
pub struct ClientStreamingCall<Req, Res> {
    request: ClientStreamingRequest<Req>,
    response: ClientStreamingResponse<Res>,
}

impl<Req: protobuf::StaticMessage, Res: protobuf::StaticMessage> ClientStreamingCall<Req, Res> {
    pub fn new(
        request: ClientStreamingRequest<Req>,
        response: ClientStreamingResponse<Res>,
    ) -> Self {
        Self { request, response }
    }

    pub fn context(&self) -> &ClientResponseContext {
        &self.response.context()
    }

    #[must_use]
    pub async fn send(&mut self, message: &Req) -> bool {
        self.request.send(message).await
    }

    pub async fn finish(mut self) -> Result<Res> {
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
