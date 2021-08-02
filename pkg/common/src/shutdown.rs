// Utilities for 

use std::future::Future;
use std::sync::Once;
use std::sync::Mutex;

use async_std::channel;


use crate::signals::*;
use crate::future::race;

static mut SHUTDOWN_STATE: Option<Mutex<ShutdownState>> = None;
static SHUTDOWN_STATE_INIT: Once = Once::new();

struct ShutdownState {
    /// Sending half of the channel used to notify tasks when a shutdown has occured.
    /// We don't actually send any data through the sender. Instead, when the 
    sender: Option<channel::Sender<()>>,

    /// Receiving end of the shutdown notification channel. When receiving from this channel
    /// fails/unblocks, the program is shutting down.
    receiver: channel::Receiver<()>
}

fn get_shutdown_state() -> &'static Mutex<ShutdownState> {
    unsafe {
        SHUTDOWN_STATE_INIT.call_once(|| {
            let (sender, receiver) = channel::bounded(1);

            SHUTDOWN_STATE = Some(Mutex::new(ShutdownState {
                sender: Some(sender),
                receiver
            }));

            async_std::task::spawn(signal_waiter());
        });

        SHUTDOWN_STATE.as_ref().unwrap()
    }
}

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
impl<F: 'static + Send + Sync + FnMut() -> Fut, Fut: std::future::Future<Output=()> + Send> ShutdownHandler for F {
    async fn shutdown(&mut self) {
        (self)().await
    }
}

pub trait IntoShutdownHandler {
    fn into_shutdown_handler(self) -> Box<dyn ShutdownHandler>; 
}

impl<T: ShutdownHandler> IntoShutdownHandler for T {
    fn into_shutdown_handler(self) -> Box<dyn ShutdownHandler> {
        Box::new(self)
    }
}

// impl<F: 'static + Send + Sync + FnOnce() -> Fut, Fut: std::future::Future<Output=()> + Send> IntoShutdownHandler for F {
//     fn into_shutdown_handler(self) -> Box<dyn ShutdownHandler> {
//         todo!()
//     }
// }

pub fn new_shutdown_token() -> impl Future<Output=()> {
    let receiver = {
        let shutdown_state = get_shutdown_state().lock().unwrap();
        shutdown_state.receiver.clone()
    };

    async move {
        let _ = receiver.recv().await;
    }
}

// pub async fn register_shutdown_handler<H: IntoShutdownHandler>(handler: H) {
//     register_shutdown_handler_impl(handler.into_shutdown_handler()).await;
// }

// async fn register_shutdown_handler_impl(mut handler: Box<dyn ShutdownHandler>) {
//     let mut shutdown_state = get_shutdown_state().lock().await;
//     if shutdown_state.is_shutting_down {
//         drop(shutdown_state);
//         handler.shutdown().await;
//         return;
//     }

//     shutdown_state.handlers.push(handler);
// }