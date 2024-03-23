// Utilities for

use alloc::boxed::Box;
use alloc::vec::Vec;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;

use crate::cancellation::CancellationToken;
use crate::channel;
use crate::future::race;
use crate::signals::*;
use crate::spawn;

static mut SHUTDOWN_STATE: Option<std::sync::Mutex<ShutdownState>> = None;
static SHUTDOWN_STATE_INIT: Once = Once::new();

struct ShutdownState {
    /// Sending half of the channel used to notify tasks when a shutdown has
    /// occured. We don't actually send any data through the sender.
    /// Instead, when the
    sender: Option<channel::Sender<()>>,

    /// Receiving end of the shutdown notification channel. When receiving from
    /// this channel fails/unblocks, the program is shutting down.
    receiver: channel::Receiver<()>,

    num_tokens: usize,

    completion_waiters: Vec<channel::Sender<()>>,
}

fn get_shutdown_state() -> &'static std::sync::Mutex<ShutdownState> {
    unsafe {
        SHUTDOWN_STATE_INIT.call_once(|| {
            let (sender, receiver) = channel::bounded(1);

            SHUTDOWN_STATE = Some(Mutex::new(ShutdownState {
                sender: Some(sender),
                receiver,
                num_tokens: 0,
                completion_waiters: vec![],
            }));

            spawn(signal_waiter());
        });

        SHUTDOWN_STATE.as_ref().unwrap()
    }
}

/// Background task used to block until a unix shutdown signal is received and
/// then notify all subscribers.
async fn signal_waiter() {
    let mut sigint_handler = register_signal_handler(Signal::SIGINT).unwrap();
    let mut sigterm_handler = register_signal_handler(Signal::SIGTERM).unwrap();

    race(sigint_handler.recv(), sigterm_handler.recv()).await;

    trigger_shutdown();
}

#[async_trait]
pub trait ShutdownHandler: Send + Sync + 'static {
    async fn shutdown(&mut self);
}

#[async_trait]
impl<F: 'static + Send + Sync + FnMut() -> Fut, Fut: std::future::Future<Output = ()> + Send>
    ShutdownHandler for F
{
    async fn shutdown(&mut self) {
        (self)().await
    }
}

struct ShutdownToken {
    receiver: channel::Receiver<()>,
}

impl Drop for ShutdownToken {
    fn drop(&mut self) {
        let mut shutdown_state = get_shutdown_state().lock().unwrap();
        shutdown_state.num_tokens -= 1;
        if shutdown_state.num_tokens == 0 {
            // Close all the channels.
            shutdown_state.completion_waiters.clear();
        }
    }
}

#[async_trait]
impl CancellationToken for ShutdownToken {
    async fn is_cancelled(&self) -> bool {
        self.receiver.is_closed()
    }

    async fn wait_for_cancellation(&self) {
        let _ = self.receiver.recv().await;
    }
}

/// Gets a token which will can be used to wait for the program to enter a
/// graceful shutdown mode. Once unblocked, the user should perform any needed
/// clean up and stop running.
///
/// NOTE: There is one global shutdown state for the entire program.
pub fn new_shutdown_token() -> Arc<dyn CancellationToken> {
    let receiver = {
        let mut shutdown_state = get_shutdown_state().lock().unwrap();
        shutdown_state.num_tokens += 1;
        shutdown_state.receiver.clone()
    };

    Arc::new(ShutdownToken { receiver })
}

/// Explicitly indicate that the application is shutting down. Once triggered,
/// all shutdown tokens will unblock until the end of the program.
pub fn trigger_shutdown() {
    let mut shutdown_state = get_shutdown_state().lock().unwrap();
    shutdown_state.sender.take();
}

/// Blocks until all shutdown tokens which have been handled out have been
/// dropped.
///
/// NOTE: This assumes that shutdown is triggered by something else.
pub async fn wait_for_shutdowns() {
    let (sender, receiver) = channel::bounded(1);

    {
        let mut shutdown_state = get_shutdown_state().lock().unwrap();
        if shutdown_state.num_tokens == 0 {
            return;
        }

        shutdown_state.completion_waiters.push(sender);
    }

    let _ = receiver.recv().await;
}
