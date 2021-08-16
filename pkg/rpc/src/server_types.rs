use std::marker::PhantomData;

use common::async_std::channel;
use common::errors::*;
use http::Body;

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

pub struct ServerStreamRequest<T> {
    pub(crate) request_body: Box<dyn Body>,
    pub(crate) context: ServerRequestContext,
    pub(crate) phantom_t: PhantomData<T>,
}

impl ServerStreamRequest<()> {
    pub fn into<T: protobuf::Message>(self) -> ServerStreamRequest<T> {
        ServerStreamRequest {
            request_body: self.request_body,
            context: self.context,
            phantom_t: PhantomData,
        }
    }

    /// NOTE: It's only valid to call this before using recv().
    pub async fn into_unary<T: protobuf::Message>(self) -> Result<ServerRequest<T>> {
        // TODO: Change all of these to RPC errors.

        let mut stream = self.into::<T>();

        let message = stream.recv().await?.ok_or_else(|| err_msg("Empty body"))?;

        // TODO: I'm not sure if all client libraries will immediately sent the request
        // END_STREAM before getting some response?
        if !stream.recv().await?.is_none() {
            return Err(err_msg("Expected exactly one message in the request"));
        }

        Ok(ServerRequest {
            value: message,
            context: stream.context,
        })
    }
}

impl<T: protobuf::Message> ServerStreamRequest<T> {
    pub async fn recv(&mut self) -> Result<Option<T>> {
        let mut message_reader = MessageReader::new(self.request_body.as_mut());

        let data = message_reader.read().await?;

        let message = {
            if let Some(data) = data {
                Some(T::parse(&data).map_err(|_| {
                    crate::Status::invalid_argument("Failed to parse request proto.")
                })?)
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

    /// Whether or not we have sent the head of the request yet.
    pub(crate) head_sent: &'a mut bool,

    pub(crate) sender: channel::Sender<ServerStreamResponseEvent>,
    pub(crate) phantom_t: PhantomData<T>,
}

impl<'a> ServerStreamResponse<'a, ()> {
    pub fn into<T: protobuf::Message>(self) -> ServerStreamResponse<'a, T> {
        ServerStreamResponse {
            context: self.context,
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

impl<'a, T: protobuf::Message> ServerStreamResponse<'a, T> {
    /// Enqueue a single message to be sent back to the client.
    ///
    /// Once the first message is enqueued, you can no longer append any head
    /// metadata. NOTE: This will block based on connection level flow
    /// control.
    pub async fn send(&mut self, message: T) -> Result<()> {
        if !*self.head_sent {
            *self.head_sent = true;
            // TODO: Make this more efficient?
            self.sender
                .send(ServerStreamResponseEvent::Head(
                    self.context.metadata.head_metadata.clone(),
                ))
                .await?;
        }

        let data = message.serialize()?;
        self.sender
            .send(ServerStreamResponseEvent::Message(data))
            .await?;
        Ok(())
    }
}

pub(crate) enum ServerStreamResponseEvent {
    Head(Metadata),
    Message(Vec<u8>),
    Trailers(Result<()>, Metadata),
}
