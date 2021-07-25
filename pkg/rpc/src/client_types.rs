use std::marker::PhantomData;

use common::errors::*;
use common::io::Readable;
use common::bytes::Bytes;
use common::async_std::channel;
use common::async_std::task;
use common::task::ChildTask;
use http::header::CONTENT_TYPE;

use crate::metadata::*;
use crate::status::*;
use crate::message::*;
use crate::constants::GRPC_PROTO_TYPE;


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


/// Used by an RPC client to specify how a single RPC should be sent and what metadata
/// should be sent along with the RPC. 
#[derive(Default)]
pub struct ClientRequestContext {
    pub metadata: Metadata,
    pub idempotent: bool,
    pub fail_fast: bool
    // TODO: Deadline
}

#[derive(Default)]
pub struct ClientResponseContext {
    /// Metadata received from the server.
    ///
    /// NOTE: This may not be complete if we received an invalid response from the server.
    /// Implementations should be resilient to this missing some keys.
    pub metadata: ResponseMetadata
}


/// Response returned from an RPC with unary request and unary resposne.
pub struct ClientResponse<T> {
    pub context: ClientResponseContext,

    /// If the RPC was successful with an OK status, then this will contain the value of the
    /// response message, otherwise this will contain an error. In the latter case, this will
    /// usually be an rpc::Status if we successfully connected to the remote server.
    ///
    /// NOTE: In some cases it is possible that we receive a response value and a non-OK status.
    /// In these cases we will ignore the value.
    pub result: Result<T>,
}

/// Interface for sending a stream of messages to the server.
///
/// NOTE: Droping the ClientStreamingRequest before close() is executed will mark the request
/// stream is incomplete.
///
/// TODO: Double check that any failure in the ClientStreamingRequest is propagated to the
/// ClientStreamingResponse and vice versa.
pub struct ClientStreamingRequest<T> {
    sender: channel::Sender<Result<Option<Bytes>>>,
    phantom_t: PhantomData<T>,
}

impl ClientStreamingRequest<()> {
    pub(crate) fn new(sender: channel::Sender<Result<Option<Bytes>>>) -> Self {
        Self {
            sender,
            phantom_t: PhantomData
        }
    }

    pub(crate) fn into<T: protobuf::Message>(self) -> ClientStreamingRequest<T> {
        ClientStreamingRequest {
            sender: self.sender,
            phantom_t: PhantomData
        }
    }
}

impl<T: protobuf::Message> ClientStreamingRequest<T> {
    /// Returns whether or not the message was sent. If not, then the connection was broken
    /// and the client should check the finish() method on the other end.
    #[must_use]
    pub async fn send(&mut self, message: &T) -> bool {
        // TODO: Verify that we see this error propagated to the response side.
        let data = match message.serialize() {
            Ok(v) => Ok(Some(v.into())),
            Err(e) => Err(e)
        };

        self.sender.send(data).await.is_ok()
    }

    /// Call after sending all messages to the server to indicate that no more messages will
    /// be sent for the current RPC.
    pub async fn close(&mut self) {
        let _ = self.sender.send(Ok(None)).await;
    }
}

/// Response returned by an RPC with a unary request and streaming response.
///
/// TODO: Check that our HTTP2 implementation will close the entire stream once the Server closes its
/// stream (as there isn't any good usecase for continuing to send client bytes in this case).
pub struct ClientStreamingResponse<Res> {
    pub context: ClientResponseContext,

    state: Option<ClientStreamingResponseState>,

    phantom_t: PhantomData<Res>
}

enum ClientStreamingResponseState {
    Head(ChildTask<Result<http::Response>>),
    /// In this state, we have 
    Body(Box<dyn http::Body>),
    Trailers(Box<dyn http::Body>),
    Error(Error),
}

impl<Res> ClientStreamingResponse<Res> {
    pub(crate) fn from_response<F: 'static + Send + std::future::Future<Output=Result<http::Response>>>(
        response: F
    ) -> Self {
        Self {
            context: ClientResponseContext {
                metadata: ResponseMetadata {
                    head_metadata: Metadata::default(),
                    trailer_metadata: Metadata::default()
                },
            },
            state: Some(ClientStreamingResponseState::Head(ChildTask::spawn(response))),
            phantom_t: PhantomData
        }
    }

    pub(crate) fn from_error(error: Error) -> Self {
        Self {
            context: ClientResponseContext::default(),
            state: Some(ClientStreamingResponseState::Error(error)),
            phantom_t: PhantomData
        }
    }
}

impl ClientStreamingResponse<()> {
    pub(crate) fn into<Res: protobuf::Message>(self) -> ClientStreamingResponse<Res> {
        ClientStreamingResponse {
            context: self.context,
            state: self.state,
            phantom_t: PhantomData
        }
    }
}

impl<Res: protobuf::Message> ClientStreamingResponse<Res> {

    // TODO: Consider adding a method to wait for initial metadata?

    pub async fn recv(&mut self) -> Option<Res> {
        loop {
            return match self.state.take() {
                Some(ClientStreamingResponseState::Head(response)) => {
                    if let Err(e) = self.recv_head(response).await {
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
                None => None
            }
        }
    }

    async fn recv_head(&mut self, response: ChildTask<Result<http::Response>>) -> Result<()> {
        let response = response.join().await?;

        if response.head.status_code != http::status_code::OK {
            return Err(err_msg("Server responded with non-OK status"));
        }

        let response_type = response.head.headers.find_one(CONTENT_TYPE)?.value.to_ascii_str()?;
        if response_type != GRPC_PROTO_TYPE {
            return Err(format_err!("Received RPC response with unknown Content-Type: {}", response_type));
        }

        self.context.metadata.head_metadata = Metadata::from_headers(&response.head.headers)?;

        self.state = Some(ClientStreamingResponseState::Body(response.body));

        println!("GOT HEAD");

        Ok(())
    }

    async fn recv_body(&mut self, mut body: Box<dyn http::Body>) -> Result<Option<Res>> {
        let mut reader = MessageReader::new(body.as_mut());

        let message_bytes = reader.read().await?;

        if let Some(data) = message_bytes {
            let message = Res::parse(&data)?;

            // Keep trying to read more messages.
            self.state = Some(ClientStreamingResponseState::Body(body));

            Ok(Some(message))
        } else {
            self.state = Some(ClientStreamingResponseState::Trailers(body));
            Ok(None)
        }
    }

    /// Call after the response is fully read to receive the trailer metadata and RPC status.
    pub async fn finish(&mut self) -> Result<()> {
        let state = self.state.take()
            .ok_or_else(|| err_msg("Response in invalid state"))?;
        
        match state {
            ClientStreamingResponseState::Head(_) | ClientStreamingResponseState::Body(_) => {
                Err(err_msg("Response body hasn't been fully read yet"))
            }
            ClientStreamingResponseState::Error(e) => {
                Err(e)
            }
            ClientStreamingResponseState::Trailers(mut body) => {
                let trailers = body.trailers().await?
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

/// Interface for sending an RPC with a streaming request and returning a unary response.
pub struct ClientStreamingCall<Req, Res> {
    request: ClientStreamingRequest<Req>,
    response: ClientStreamingResponse<Res>
}

impl<Req: protobuf::Message, Res: protobuf::Message> ClientStreamingCall<Req, Res> {
    pub fn new(request: ClientStreamingRequest<Req>, response: ClientStreamingResponse<Res>) -> Self {
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