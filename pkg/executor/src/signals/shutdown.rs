// Utilities for

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use std::sync::Mutex;
use std::sync::Once;

use crate::cancellation::CancellationToken;
use crate::channel;
use crate::future::race;
use crate::signals::*;
use crate::spawn;

static mut SHUTDOWN_STATE: Option<Mutex<ShutdownState>> = None;
static SHUTDOWN_STATE_INIT: Once = Once::new();

struct ShutdownState {
    /// Sending half of the channel used to notify tasks when a shutdown has
    /// occured. We don't actually send any data through the sender.
    /// Instead, when the
    sender: Option<channel::Sender<()>>,

    /// Receiving end of the shutdown notification channel. When receiving from
    /// this channel fails/unblocks, the program is shutting down.
    receiver: channel::Receiver<()>,
}

fn get_shutdown_state() -> &'static Mutex<ShutdownState> {
    unsafe {
        SHUTDOWN_STATE_INIT.call_once(|| {
            let (sender, receiver) = channel::bounded(1);

            SHUTDOWN_STATE = Some(Mutex::new(ShutdownState {
                sender: Some(sender),
                receiver,
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

    let mut shutdown_state = get_shutdown_state().lock().unwrap();
    shutdown_state.sender.take();
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

#[async_trait]
impl CancellationToken for ShutdownToken {
    async fn wait(&self) {
        let _ = self.receiver.recv().await;
    }
}

pub fn new_shutdown_token() -> Box<dyn CancellationToken> {
    let receiver = {
        let shutdown_state = get_shutdown_state().lock().unwrap();
        shutdown_state.receiver.clone()
    };

    Box::new(ShutdownToken { receiver })
}
