use common::bytes::Buf;
use common::bytes::Bytes;
use common::errors::*;

use crate::client_types::*;

/// Interface like a network socket for invoking a remote RPC service.
///
/// Unlike a plain network socket, a Channel is expected to support multiplexing
/// of concurrent requests and transmission of well framed messages and other
/// metadata.
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
    ) -> (ClientStreamingRequest<()>, ClientStreamingResponse<()>);
}

impl dyn Channel {
    /// ONLY FOR GENERATED CODE.
    pub async fn call_stream_stream<Req: protobuf::StaticMessage, Res: protobuf::StaticMessage>(
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

    /// ONLY FOR GENERATED CODE.
    pub async fn call_unary_stream<Req: protobuf::StaticMessage, Res: protobuf::StaticMessage>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_value: &Req,
    ) -> ClientStreamingResponse<Res> {
        let (mut req, res) = self
            .call_stream_stream(service_name, method_name, request_context)
            .await;

        // Batch the send() and close() calls.
        req.cork();

        // NOTE: If the send failed, then the response should get an error.
        let _ = req.send(request_value).await;
        req.close().await;

        res
    }

    /// ONLY FOR GENERATED CODE.
    pub async fn call_unary_unary<Req: protobuf::StaticMessage, Res: protobuf::StaticMessage>(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_value: &Req,
    ) -> ClientResponse<Res> {
        // Enable retrying for unary requests.
        let mut request_context = request_context.clone();
        request_context.buffer_full_response = true;

        let mut response = self
            .call_unary_stream(service_name, method_name, &request_context, request_value)
            .await;
        let response_value = self.call_unary_unary_impl(&mut response).await;

        ClientResponse {
            context: response.context,
            result: response_value,
        }
    }

    async fn call_unary_unary_impl<Res: protobuf::StaticMessage>(
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

    /// ONLY FOR GENERATED CODE.
    pub async fn call_stream_unary<Req: protobuf::StaticMessage, Res: protobuf::StaticMessage>(
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
