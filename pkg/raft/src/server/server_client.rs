use std::sync::Arc;

use common::errors::*;
use common::io::{IoError, IoErrorKind};
use executor::channel;
use executor::child_task::ChildTask;
use raft_proto::raft::*;

/// Instance of a
///
///
/// Wrapper around a Consensus stub for sending serialized AppendEntriesRequest
/// protos and then getting back the corresponding responses.
pub struct ServerClient {
    stub: Arc<ConsensusStub>,

    sender: channel::Sender<RequestWithCallback>,
    child: ChildTask,
}

struct RequestWithCallback {
    request: AppendEntriesRequest,
    callback: channel::oneshot::Sender<Result<AppendEntriesResponse>>,
}

// TODO: Maintain a backoff if there are any attempts to make requests via the
// client's stub.

impl ServerClient {
    pub fn new(stub: Arc<ConsensusStub>, request_context: rpc::ClientRequestContext) -> Self {
        // NOTE: The size of this channel is small to ensure that the stub's flow
        // control is used to limit the rate of requests.
        let (sender, receiver) = channel::bounded(1);

        Self {
            stub: stub.clone(),
            sender,
            child: ChildTask::spawn(Self::append_entries_runner_task(
                stub,
                request_context,
                receiver,
            )),
        }
    }

    pub fn stub(&self) -> &ConsensusStub {
        &self.stub
    }

    pub async fn send_append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse> {
        let (sender, receiver) = channel::oneshot::channel();

        // NOTE: Errors will be noticed when pulling from the reciever
        let _ = self
            .sender
            .send(RequestWithCallback {
                request,
                callback: sender,
            })
            .await;

        let res = receiver.recv().await.map_err(|_| {
            Error::from(IoError::new(
                IoErrorKind::Cancelled,
                "AppendEntries stream was aborted",
            ))
        })??;

        Ok(res)
    }

    async fn append_entries_runner_task(
        stub: Arc<ConsensusStub>,
        request_context: rpc::ClientRequestContext,
        receiver: channel::Receiver<RequestWithCallback>,
    ) {
        loop {
            // let mut pending_response = vec![];

            // TODO: Add some exponential backoff to this.
            let (mut req_stream, mut res_stream) = stub.AppendEntries(&request_context).await;

            loop {
                let req = match receiver.recv().await {
                    Ok(v) => v,
                    Err(_) => break,
                };

                let success = req_stream.send(&req.request).await;

                let res = res_stream.recv().await;

                if !success || !res.is_some() {
                    req_stream.close().await;

                    let err = res_stream
                        .finish()
                        .await
                        .and_then(|_| -> Result<()> {
                            Err(err_msg("No value returned for AppendEntries request"))
                        })
                        .unwrap_err();

                    let _ = req.callback.send(Err(err));
                    break;
                }

                let res = res.unwrap();

                let _ = req.callback.send(Ok(res));
            }

            // - Timeout for the head request to avoid things taking too long
            //   (if one times out, time out them all)

            // One of three things could happen:
            // - Done appending an entry
            // - receiver has stuff.
            // - response has stuff.

            // - response may die in which case we need to kill

            /*
            How to deal with graceful shutdown?
            - Don't care. Just treat as an error and let raft backoff.
            */
        }
    }

    /*


    */
}
