// Shared utility for assigning signal handlers.
// This should be the only code that modifies the signals directly via syscalls. We use this fact
// to gurantee that at most one signal handler is configured at a time. Multiplexing must be
// done at a higher level.

use std::collections::HashMap;
use std::sync::Once;
use std::sync::Mutex;

use async_std::channel;
use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, sigaction};
pub use nix::sys::signal::Signal;

use crate::errors::*;

static mut SIGNALS_STATE: Option<Mutex<SignalsState>> = None;
static SIGNALS_STATE_INIT: Once = Once::new();

/// Process-wide state of how the different signals are configured.
struct SignalsState {
    senders: HashMap<libc::c_int, channel::Sender<()>>
}

fn get_signals_state() -> &'static Mutex<SignalsState> {
    unsafe {
        SIGNALS_STATE_INIT.call_once(|| {
            SIGNALS_STATE = Some(Mutex::new(SignalsState {
                senders: HashMap::new()
            }));
        });

        SIGNALS_STATE.as_ref().unwrap()
    }
}

extern "C" fn signal_handler(signal: libc::c_int) {
    let signals_state = get_signals_state().lock().unwrap();
    if let Some(sender) = signals_state.senders.get(&signal) {
        let _ = sender.try_send(());
    }
}

pub struct SignalReceiver {
    signal: Signal,
    receiver: channel::Receiver<()>
}

impl Drop for SignalReceiver {
    fn drop(&mut self) {
        // Reset the signal handler back to the default value.
        let action = SigAction::new(
            SigHandler::SigDfl, 
            SaFlags::empty(), 
            SigSet::empty());
        unsafe { sigaction(self.signal, &action) }.unwrap();

        let mut signals_state = get_signals_state().lock().unwrap();
        signals_state.senders.remove(&(self.signal as libc::c_int));
    }
}

impl SignalReceiver {
    pub async fn recv(&mut self) {
        self.receiver.recv().await.unwrap();
    }
}

/// Registers a signal handler with the OS to receive the given signal.
///
/// The caller can be notified of signal receival by calling .recv() on
/// the returned value. An error will be returned if the signal has already been
/// registered. 
pub fn register_signal_handler(signal: Signal) -> Result<SignalReceiver> {
    let (sender, receiver) = channel::bounded(1);
    
    let signal_num= signal as libc::c_int;
    {
        let mut signals_state = get_signals_state().lock().unwrap();
        if signals_state.senders.contains_key(&signal_num) {
            return Err(err_msg("Signal already registered"));
        }

        signals_state.senders.insert(signal_num, sender);
    }

    // Register the signal handler with the OS.
    // NOTE: The sigaction() syscall is recommended over signal(). 
    let action = SigAction::new(
        SigHandler::Handler(signal_handler), 
        SaFlags::empty(), 
        SigSet::empty());
    unsafe { sigaction(signal, &action) }?;

    Ok(SignalReceiver {
        signal, receiver
    })
}
