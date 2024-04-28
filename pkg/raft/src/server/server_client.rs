use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use common::futures::{pin_mut, select, FutureExt};
use common::io::{IoError, IoErrorKind};
use executor::channel::oneshot;
use executor::child_task::ChildTask;
use executor::sync::AsyncVariable;
use executor::{channel, lock};
use raft_proto::raft::*;

/// Timeout for the first AppendEntries request in line waiting for a response.
const APPEND_ENTRIES_TIMEOUT: Duration = Duration::from_secs(2);

/// Instance of a
///
///
/// Wrapper around a Consensus stub for sending serialized AppendEntriesRequest
/// protos and then getting back the corresponding responses.
pub struct ServerClient {
    shared: Arc<Shared>,
    append_entries_task: ChildTask,
}

struct Shared {
    stub: Arc<ConsensusStub>,
    append_entries_queue: AsyncVariable<AppendEntriesQueue>,
}

#[derive(Default)]
struct AppendEntriesQueue {
    /// Index in 'requests' of the next request that needs to be sent.
    next_index: usize,

    /// List of requests pending a response from the remote server.
    /// - Requests are sent in the same order as in this list.
    /// - Requests are popped from the front of the list after a response is
    ///   received.
    requests: VecDeque<RequestWithCallback>,
}

struct RequestWithCallback {
    /// The sending thread takes this value and leaves beyond 'None' when the
    /// request is ready to be sent.
    request: Option<AppendEntriesRequest>,

    callback: channel::oneshot::Sender<Result<AppendEntriesResponse>>,
    enqueue_time: Instant,
}

// TODO: Maintain a backoff if there are any attempts to make requests via the
// client's stub.
// ^ Yes. (eventually would want to replace with some smarter mechanism)
// But should be per-request type.
// - Don't want heartbeat requests to become broken by other types of requests.
// Ideally mark all of them internally with wait_for_ready to allow blocking the
// requests if we need to throttle.

impl ServerClient {
    pub fn new(stub: Arc<ConsensusStub>, request_context: rpc::ClientRequestContext) -> Self {
        let shared = Arc::new(Shared {
            stub,
            append_entries_queue: AsyncVariable::default(),
        });

        Self {
            shared: shared.clone(),
            append_entries_task: ChildTask::spawn(Self::append_entries_runner_task(
                shared,
                request_context,
            )),
        }
    }

    pub fn stub(&self) -> &ConsensusStub {
        &self.shared.stub
    }

    /// Enqueues an AppendEntriesRequest to be sent to the remote server.
    ///
    /// The request will be sent before any previous calls to
    /// enqueue_append_entries
    pub async fn enqueue_append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> impl Future<Output = Result<AppendEntriesResponse>> {
        let (sender, receiver) = channel::oneshot::channel();

        lock!(
            queue <= self.shared.append_entries_queue.lock().await.unwrap(),
            {
                queue.requests.push_back(RequestWithCallback {
                    request: Some(request),
                    callback: sender,
                    enqueue_time: Instant::now(),
                });

                queue.notify_all();
            }
        );

        async move {
            let res = receiver.recv().await.map_err(|_| {
                Error::from(IoError::new(
                    IoErrorKind::Cancelled,
                    "AppendEntries stream was aborted",
                ))
            })??;

            Ok(res)
        }
    }

    async fn append_entries_runner_task(
        shared: Arc<Shared>,
        mut request_context: rpc::ClientRequestContext,
    ) {
        // Since we continuously restart the RPC, we should wait if the connection isn't
        // ready.
        request_context.http.wait_for_ready = true;

        // Each iteration is one attempt at sending an AppendEntries stream to the
        // remote server.
        loop {
            let (result_sender, result_receiver) = oneshot::channel();

            let shared2 = shared.clone();
            let request_context2 = request_context.clone();
            let streamer = ChildTask::spawn(async move {
                result_sender.send(Self::append_entries_streamer(shared2, request_context2).await);
            });

            let result = result_receiver.recv();
            pin_mut!(result);

            loop {
                let r = executor::timeout(Duration::from_millis(200), &mut result).await;

                match r {
                    Ok(result) => {
                        // The child task stopped running
                        // eprintln!("AppendEntries stream stopped with: {:?}",
                        // result);

                        // NOTE: Polling the 'result' future again will result in undefined
                        // behavior.
                        break;
                    }
                    Err(_) => {
                        // Timeout

                        let now = Instant::now();
                        let stop = lock!(
                            queue <= shared.append_entries_queue.lock().await.unwrap(),
                            {
                                match queue.requests.get(0) {
                                    Some(entry) => {
                                        now - entry.enqueue_time >= APPEND_ENTRIES_TIMEOUT
                                    }
                                    None => false,
                                }
                            }
                        );

                        if stop {
                            // eprintln!("AppendEntries timed out");
                            break;
                        }
                    }
                }
            }

            // Ensure that the streamer is no longer running to ensure that we can safely
            // mutate the queue.
            streamer.cancel().await;

            // Clear the queue.
            lock!(
                queue <= shared.append_entries_queue.lock().await.unwrap(),
                {
                    // NOTE: There is no point in preserving requests that haven't been sent yet
                    // since they likely can't be appended if previous requests failed.
                    queue.next_index = 0;
                    for entry in queue.requests.drain(..) {
                        entry
                            .callback
                            .send(Err(err_msg("AppendEntries stream timed out or failed.")));
                    }
                }
            );

            // Ensure that the task can get cancelled without infinite looping.
            // TODO: Ensure we do this in all infinite loops.
            executor::yield_now().await;
        }
    }

    async fn append_entries_streamer(
        shared: Arc<Shared>,
        request_context: rpc::ClientRequestContext,
    ) -> Result<()> {
        let (mut req_stream, mut res_stream) = shared.stub.AppendEntries(&request_context).await;

        /*
        Main cases to think about:

        - RPC returns an error
            - IDeally still gracefully close everything.

        - RPC stops getting sent to the remote side.

        - During shutdown, the server needs to return an error to the client since the request duration is unbounded.

        */

        let shared2 = shared.clone();
        let sender = ChildTask::spawn(async move {
            loop {
                let request;
                loop {
                    let mut queue = shared2.append_entries_queue.lock().await.unwrap().enter();
                    if queue.requests.len() <= queue.next_index {
                        queue.wait().await;
                        continue;
                    }

                    let idx = queue.next_index;
                    request = queue.requests[idx].request.take().unwrap();
                    queue.next_index += 1;
                    queue.exit();
                    break;
                }

                let success = req_stream.send(&request).await;
                if !success {
                    // TODO: Verify that if this happens, then recv() is also guaranteed to return
                    // None soon.

                    break;
                }
            }

            // TODO: verify that it isn't necessary for us to ever call
            // req_stream.close()
        });

        loop {
            let res = match res_stream.recv().await {
                Some(v) => v,
                None => break,
            };

            lock!(
                queue <= shared.append_entries_queue.lock().await.unwrap(),
                {
                    if queue.next_index == 0 {
                        return Err(err_msg("Received response when no request was sent."));
                    }

                    let entry = queue.requests.pop_front().unwrap();
                    queue.next_index -= 1;

                    let _ = entry.callback.send(Ok(res));

                    Ok(())
                }
            )?;
        }

        res_stream.finish().await
    }
}
