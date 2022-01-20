use std::marker::PhantomData;

use common::async_std::channel;
use common::bytes::Bytes;
use common::errors::*;
use http::Body;
use protobuf_json::{MessageJsonParser, MessageJsonSerialize};

use crate::media_type::{RPCMediaSerialization, RPCMediaType};
use crate::message::MessageReader;
use crate::metadata::*;

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

pub struct ServerRequest<T: protobuf::Message> {
    pub value: T,
    pub context: ServerRequestContext,
}

impl<T: protobuf::Message> std::ops::Deref for ServerRequest<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// RPC request received by a server consisting of zero or more messages.
///
/// Internally this is implemented by reading from an http::Body.
pub struct ServerStreamRequest<T> {
    request_body: Box<dyn Body>,
    request_type: RPCMediaType,
    context: ServerRequestContext,
    phantom_t: PhantomData<T>,
}

impl ServerStreamRequest<()> {
    pub(crate) fn new(
        request_body: Box<dyn Body>,
        request_type: RPCMediaType,
        context: ServerRequestContext,
    ) -> Self {
        Self {
            request_body,
            request_type,
            context,
            phantom_t: PhantomData,
        }
    }

    pub fn into<T: protobuf::Message>(self) -> ServerStreamRequest<T> {
        ServerStreamRequest {
            request_body: self.request_body,
            request_type: self.request_type,
            context: self.context,
            phantom_t: PhantomData,
        }
    }

    /// NOTE: It's only valid to call this before using recv().
    pub async fn into_unary<T: protobuf::Message + Default>(self) -> Result<ServerRequest<T>> {
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

impl<T: protobuf::Message + Default> ServerStreamRequest<T> {
    pub async fn recv(&mut self) -> Result<Option<T>> {
        let data = self.recv_bytes().await?;

        let message = {
            if let Some(data) = data {
                let value = match self.request_type.serialization {
                    RPCMediaSerialization::Proto => T::parse(&data),
                    RPCMediaSerialization::JSON => {
                        let options = protobuf_json::ParserOptions {
                            ignore_unknown_fields: true,
                        };

                        std::str::from_utf8(data.as_ref())
                            .map_err(|e| Error::from(e))
                            .and_then(|s| T::parse_json(s, &options))
                    }
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

pub struct ServerResponse<'a, T: protobuf::Message> {
    /// Value to be returned to the client. Only fully returned if the response
    pub value: T,
    pub context: &'a mut ServerResponseContext,
}

impl<'a, T: protobuf::Message> ServerResponse<'a, T> {
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<'a, T: protobuf::Message> std::ops::Deref for ServerResponse<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T: protobuf::Message> std::ops::DerefMut for ServerResponse<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

pub struct ServerStreamResponse<'a, T> {
    pub context: &'a mut ServerResponseContext,

    pub(crate) response_type: RPCMediaType,

    /// Whether or not we have sent the head of the request yet.
    pub(crate) head_sent: &'a mut bool,

    pub(crate) sender: channel::Sender<ServerStreamResponseEvent>,
    pub(crate) phantom_t: PhantomData<T>,
}

impl<'a> ServerStreamResponse<'a, ()> {
    pub fn into<T: protobuf::Message>(self) -> ServerStreamResponse<'a, T> {
        ServerStreamResponse {
            context: self.context,
            response_type: self.response_type,
            head_sent: self.head_sent,
            sender: self.sender,
            phantom_t: PhantomData,
        }
    }

    /// NOTE: Later the value must be given back to the stream.
    pub fn new_unary<'b, T: protobuf::Message + Default>(&'b mut self) -> ServerResponse<'b, T> {
        ServerResponse {
            value: T::default(),
            context: self.context,
        }
    }
}

impl<'a, T> ServerStreamResponse<'a, T> {
    pub async fn send_head(&mut self) -> Result<()> {
        if !*self.head_sent {
            *self.head_sent = true;
            // TODO: Make this more efficient?
            self.sender
                .send(ServerStreamResponseEvent::Head(
                    self.context.metadata.head_metadata.clone(),
                ))
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn send_bytes(&mut self, data: Bytes) -> Result<()> {
        self.send_head().await?;

        self.sender
            .send(ServerStreamResponseEvent::Message(data))
            .await?;
        Ok(())
    }
}

impl<'a, T: protobuf::Message> ServerStreamResponse<'a, T> {
    /// Enqueue a single message to be sent back to the client.
    ///
    /// Once the first message is enqueued, you can no longer append any head
    /// metadata. NOTE: This will block based on connection level flow
    /// control.
    pub async fn send(&mut self, message: T) -> Result<()> {
        let data = match self.response_type.serialization {
            RPCMediaSerialization::Proto => message.serialize()?,
            RPCMediaSerialization::JSON => Vec::from(message.serialize_json()),
        };
        self.send_bytes(data.into()).await
    }
}

pub(crate) enum ServerStreamResponseEvent {
    Head(Metadata),
    Message(Bytes),
    Trailers(Result<()>, Metadata),
}
