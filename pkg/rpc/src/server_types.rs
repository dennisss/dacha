use std::marker::PhantomData;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::channel::spsc;
use http::Body;
use protobuf_json::{MessageJsonParser, MessageJsonSerialize};

use crate::media_type::{RPCMediaSerialization, RPCMediaType};
use crate::message::MessageReader;
use crate::metadata::*;

#[derive(Default)]
pub struct ServerCodecOptions {
    pub json_serializer: protobuf_json::SerializerOptions,

    pub json_parser: protobuf_json::ParserOptions,
}

/// Server-side view of information related to this request.
pub struct ServerRequestContext {
    pub metadata: Metadata, /* metadata */

                            /* connection information */

                            /* deadline (if any) */
}

#[derive(Default)]
pub struct ServerResponseContext {
    /// NOTE: We will still try to send any response metadata back to the client
    /// even if the RPC handler failed.
    pub metadata: ResponseMetadata,
}

pub struct ServerRequest<T: protobuf::StaticMessage> {
    pub value: T,
    pub context: ServerRequestContext,
}

impl<T: protobuf::StaticMessage> std::ops::Deref for ServerRequest<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: protobuf::StaticMessage> AsRef<T> for ServerRequest<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

/// RPC request received by a server consisting of zero or more messages.
///
/// Internally this is implemented by reading from an http::Body.
pub struct ServerStreamRequest<T> {
    request_body: Box<dyn Body>,
    request_type: RPCMediaType,
    codec_options: Arc<ServerCodecOptions>,
    context: ServerRequestContext,
    phantom_t: PhantomData<T>,
}

impl ServerStreamRequest<()> {
    pub(crate) fn new(
        request_body: Box<dyn Body>,
        request_type: RPCMediaType,
        codec_options: Arc<ServerCodecOptions>,
        context: ServerRequestContext,
    ) -> Self {
        Self {
            request_body,
            request_type,
            codec_options,
            context,
            phantom_t: PhantomData,
        }
    }

    pub fn into<T: protobuf::StaticMessage>(self) -> ServerStreamRequest<T> {
        ServerStreamRequest {
            request_body: self.request_body,
            request_type: self.request_type,
            codec_options: self.codec_options,
            context: self.context,
            phantom_t: PhantomData,
        }
    }

    /// NOTE: It's only valid to call this before using recv().
    pub async fn into_unary<T: protobuf::StaticMessage + Default>(
        self,
    ) -> Result<ServerRequest<T>> {
        let mut stream = self.into::<T>();

        let message = stream
            .recv()
            .await?
            .ok_or_else(|| crate::Status::unimplemented("Empty body"))?;

        // TODO: I'm not sure if all client libraries will immediately sent the request
        // END_STREAM before getting some response?
        if !stream.recv().await?.is_none() {
            return Err(crate::Status::unimplemented(
                "Expected exactly one message in the request",
            )
            .into());
        }

        Ok(ServerRequest {
            value: message,
            context: stream.context,
        })
    }
}

impl<T> ServerStreamRequest<T> {
    pub fn context(&self) -> &ServerRequestContext {
        &self.context
    }

    pub async fn recv_bytes(&mut self) -> Result<Option<Bytes>> {
        let mut message_reader = MessageReader::new(self.request_body.as_mut());
        let message = match message_reader.read().await? {
            Some(m) => m,
            None => {
                return Ok(None);
            }
        };

        if message.is_trailers {
            return Err(err_msg("Unexpected trailers received from client"));
        }

        Ok(Some(message.data))
    }
}

impl<T: protobuf::StaticMessage + Default> ServerStreamRequest<T> {
    pub async fn recv(&mut self) -> Result<Option<T>> {
        let data = self.recv_bytes().await?;

        let message = {
            if let Some(data) = data {
                let value = match self.request_type.serialization {
                    RPCMediaSerialization::Proto => T::parse(&data).map_err(|e| Error::from(e)),
                    RPCMediaSerialization::JSON => std::str::from_utf8(data.as_ref())
                        .map_err(|e| Error::from(e))
                        .and_then(|s| T::parse_json(s, &self.codec_options.json_parser)),
                }
                .map_err(|_| crate::Status::internal("Failed to parse request proto."))?;

                Some(value)
            } else {
                None
            }
        };

        Ok(message)
    }
}

/// Message response stream passed to server RPC handlers to use for streaming
/// responses back to clients.
pub struct ServerResponse<'a, T: protobuf::StaticMessage> {
    /// Value to be returned to the client. Only fully returned if the response
    pub value: T,
    pub context: &'a mut ServerResponseContext,
}

impl<'a, T: protobuf::StaticMessage> ServerResponse<'a, T> {
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<'a, T: protobuf::StaticMessage> std::ops::Deref for ServerResponse<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T: protobuf::StaticMessage> std::ops::DerefMut for ServerResponse<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<'a, T: protobuf::StaticMessage> AsMut<T> for ServerResponse<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

pub struct ServerStreamResponse<'a, T> {
    pub(crate) context: &'a mut ServerResponseContext,

    pub(crate) response_type: RPCMediaType,

    pub(crate) codec_options: Arc<ServerCodecOptions>,

    /// Whether or not we have sent the head of the request yet.
    pub(crate) head_sent: &'a mut bool,

    pub(crate) sender: &'a mut spsc::Sender<ServerStreamResponseEvent>,
    pub(crate) phantom_t: PhantomData<T>,
}

impl<'a> ServerStreamResponse<'a, ()> {
    pub fn into<T: protobuf::StaticMessage>(self) -> ServerStreamResponse<'a, T> {
        ServerStreamResponse {
            context: self.context,
            response_type: self.response_type,
            codec_options: self.codec_options,
            head_sent: self.head_sent,
            sender: self.sender,
            phantom_t: PhantomData,
        }
    }

    /// Creates a copy of this response which can be used to send responses
    /// before the original owner of the ServerStreamResponse can send more.
    ///
    /// TODO: Avoid using this.
    pub fn borrow<'b>(&'b mut self) -> ServerStreamResponse<'b, ()> {
        ServerStreamResponse {
            context: self.context,
            response_type: self.response_type.clone(),
            codec_options: self.codec_options.clone(),
            head_sent: self.head_sent,
            sender: self.sender,
            phantom_t: self.phantom_t,
        }
    }

    /// This should only be used if you only expect to send one message.
    ///
    /// TODO: Enforce the above assumption. If the assumption is broken, corking
    /// may break things.
    ///
    /// NOTE: Later the value must be given back to the stream.
    pub fn new_unary<'b, T: protobuf::StaticMessage + Default>(
        &'b mut self,
    ) -> ServerResponse<'b, T> {
        // Given that we are only sending one message, we will bundle all the messages
        // up until the trailer into one bundle. This will minimize the number of
        // packets used in HTTP2 and allows returning a Content-Length for unary
        // responses.
        self.sender.cork();

        ServerResponse {
            value: T::default(),
            context: self.context,
        }
    }
}

impl<'a, T> ServerStreamResponse<'a, T> {
    /// Requests that head metadata immediately starts getting transferred back
    /// to the client.
    ///
    /// If this is not called we will batch the headers with any response proto
    /// or status.
    ///
    /// NOTE: Servers using this setting may break the retryability of non-unary
    /// RPCs.
    pub async fn send_head(&mut self) -> Result<()> {
        if !*self.head_sent {
            *self.head_sent = true;
            // TODO: Make this more efficient?

            // TODO: If these errors are from ReceiverDropped, then we could remap them to a
            // cancellation error.
            self.sender
                .send(ServerStreamResponseEvent::Head(
                    self.context.metadata.head_metadata.clone(),
                ))
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn send_bytes(&mut self, data: Bytes) -> Result<()> {
        // TODO: Batch these messages.

        self.send_head().await?;

        // TODO: Remap these errors like on the other one.
        self.sender
            .send(ServerStreamResponseEvent::Message(data))
            .await?;
        Ok(())
    }

    pub fn context(&mut self) -> &mut ServerResponseContext {
        self.context
    }
}

impl<'a, T: protobuf::StaticMessage> ServerStreamResponse<'a, T> {
    /// Enqueue a single message to be sent back to the client.
    ///
    /// Once the first message is enqueued, you can no longer append any head
    /// metadata. NOTE: This will block based on connection level flow
    /// control.
    pub async fn send(&mut self, message: T) -> Result<()> {
        let data = match self.response_type.serialization {
            RPCMediaSerialization::Proto => message.serialize()?,
            RPCMediaSerialization::JSON => {
                Vec::from(message.serialize_json(&self.codec_options.json_serializer)?)
            }
        };
        self.send_bytes(data.into()).await
    }
}

pub(crate) enum ServerStreamResponseEvent {
    Head(Metadata),
    Message(Bytes),
    Trailers(Result<()>, Metadata),
    TrailersOnly(Result<()>, ResponseMetadata),
}
