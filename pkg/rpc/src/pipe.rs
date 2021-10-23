use common::async_std::channel;
use common::bundle::TaskResultBundle;
use common::errors::*;
use common::futures::try_join;
use common::task::ChildTask;

use crate::channel::Channel;
use crate::server_types::{ServerStreamRequest, ServerStreamResponse};
use crate::{ClientStreamingRequest, ClientStreamingResponse};

pub async fn pipe<'a>(
    client_request: ClientStreamingRequest<()>,
    client_response: ClientStreamingResponse<()>,
    server_request: ServerStreamRequest<()>,
    server_response: ServerStreamResponse<'a, ()>,
) -> Result<()> {
    try_join!(
        pipe_request(server_request, client_request),
        pipe_response(client_response, server_response)
    );

    Ok(())
}

async fn pipe_request(
    mut server_request: ServerStreamRequest<()>,
    mut client_request: ClientStreamingRequest<()>,
) -> Result<()> {
    // Assumption is that client metadata has alwready been send.

    // TODO: If we have an error while receiving bytes, send an error to the other
    // side.
    // (normally send() will have an alternative behavior if serialization failed).
    while let Some(message) = server_request.recv_bytes().await? {
        if !client_request.send_bytes(message).await {
            return Ok(());
        }
    }

    // TODO: Verify that if we don't call close(), the client request will send an
    // error to the other side.
    client_request.close().await;

    Ok(())
}

async fn pipe_response<'a>(
    mut client_response: ClientStreamingResponse<()>,
    mut server_response: ServerStreamResponse<'a, ()>,
) -> Result<()> {
    let mut first = true;
    while let Some(data) = client_response.recv_bytes().await {
        if first {
            first = false;
            server_response.context.metadata.head_metadata =
                client_response.context.metadata.head_metadata.clone();
        }

        server_response.send_bytes(data).await?;
    }

    // TODO: Ensure any rpc::Status is forwarded to the caller.
    client_response.finish().await?;

    server_response.context.metadata.trailer_metadata =
        client_response.context.metadata.trailer_metadata.clone();

    Ok(())
}
