use std::future::Future;

use common::errors::*;
use executor::{channel::oneshot, child_task::ChildTask};

use crate::Response;

/// Creates a channel for asyncronously sending an HTTP response to a client
/// when it is available.
///
/// This should only be used internally in the HTTP Connection code.
/// - When a client makes a request, the connection will create a new channel.
/// - The client will block on the ResponseReceiver
/// - The connection will send a response through the ResponseSender once it is
///   available.
///
/// Additionally this supports reverse notification of cancellation:
/// - We define a cancellation as the case where the ResponseReceiver was
///   dropped before a response was received from it.
///   - Note that once a response has been sent successfully, request
///     cancellation must be tracked through the dropping of the sent response.
///
/// TODO: Make sure we use this in the HTTP v1 code.
pub(crate) fn new_response_channel() -> (ResponseSender, ResponseReceiver) {
    // TODO: We should be able to achieve this with a single channel.
    let (sender, receiver) = oneshot::channel();
    let (cancellation_sender, cancellation_receiver) = oneshot::channel();

    (
        ResponseSender {
            sender,
            cancellation_receiver,
        },
        ResponseReceiver {
            cancellation_sender,
            receiver,
        },
    )
}

pub struct ResponseSender {
    sender: oneshot::Sender<Result<Response>>,
    cancellation_receiver: oneshot::Receiver<()>,
}

impl ResponseSender {
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub fn send(self, value: Result<Response>) {
        let _ = self.sender.send(value);
    }

    pub fn send_future<Fut: Future<Output = Result<Response>> + Send + 'static>(self, future: Fut) {
        let sender = self.sender;
        let cancellation_receiver = self.cancellation_receiver;

        let poller = executor::spawn(async move {
            let res = future.await;
            let _ = sender.send(res);
        });

        executor::spawn(async move {
            let _ = cancellation_receiver.recv().await;
            poller.cancel();
        });
    }

    /// Adds a future to execute when the ResponseReceiver is dropped without a
    /// value being received from it.
    pub fn with_cancellation_callback<
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    >(
        self,
        f: F,
    ) -> ResponseSenderWithCancellation {
        let cancellation_receiver = self.cancellation_receiver;

        ResponseSenderWithCancellation {
            sender: self.sender,
            cancellation_task: ChildTask::spawn(async move {
                let _ = cancellation_receiver.recv().await;
                f().await
            }),
        }
    }
}

pub struct ResponseSenderWithCancellation {
    sender: oneshot::Sender<Result<Response>>,
    cancellation_task: ChildTask<()>,
}

impl ResponseSenderWithCancellation {
    #[must_use]
    pub async fn send(self, value: Result<Response>) {
        // Cancel this so that a cancellation isn't triggered when the receiver
        // gracefully receives the response and drops itself.
        self.cancellation_task.cancel().await;

        // NOTE: We don't care about the result as we expect the Drop handlers in the
        // response will react appropriately.
        let _ = self.sender.send(value);
    }
}

pub struct ResponseReceiver {
    /// Channel used to signal cancellation to the ResponseSender.
    /// (signal is sent on drop)
    cancellation_sender: oneshot::Sender<()>,

    receiver: oneshot::Receiver<Result<Response>>,
}

impl ResponseReceiver {
    pub async fn recv(self) -> Result<Response> {
        let res = self
            .receiver
            .recv()
            .await
            // NOTE: If we hit this error, we can't say anything about the retryability of the
            // request.
            .map_err(|_| err_msg("Connection hung up while waiting for response"))?;
        drop(self.cancellation_sender);
        res
    }
}
