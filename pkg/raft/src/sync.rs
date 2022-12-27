use std::borrow::{Borrow, BorrowMut};
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::time::Instant;

use executor::channel;
use executor::oneshot;
use executor::sync::{Mutex, MutexGuard};

/// Creates a futures based event notification channel
///
/// In other words this is an mpsc with the addition of being able to wait for
/// the event with a maximum timeout and being able to send without consuming
/// the sender Useful for allowing one receiver to get notified of checking
/// occuring anywhere else in the system NOTE: This is a non-data carrying and
/// non-event counting channel that functions pretty much just as a dirty flag
/// which will batch all notifications into one for the next time the waiter is
/// woken up. We use an atomic boolean along side an mpsc to deduplicate
/// multiple sequential notifications occuring before the waiter gets woken up
pub fn change() -> (ChangeSender, ChangeReceiver) {
    let (sender, receiver) = channel::bounded(1);
    (ChangeSender(sender), ChangeReceiver(receiver))
}

pub struct ChangeSender(channel::Sender<()>);

impl ChangeSender {
    // TODO: In general we shouldn't use this as we are now mostly operating in a
    // threaded environment
    pub fn notify(&self) {
        if let Err(e) = self.0.try_send(()) {
            // This will fail in one of two cases:
            // 1. Either the channel is full (which is fine as we only want a
            // single notification to get scheduled at a time)
            // 2. The other side of disconnected (will be handled by some other
            // code)
        }
    }
}

pub struct ChangeReceiver(channel::Receiver<()>);

impl ChangeReceiver {
    /// Waits indefinately until the change occurs for the first time.
    pub async fn wait(&self) {
        let _ = self.0.recv().await;
    }

    pub async fn wait_until(&self, until: Instant) {
        let now = Instant::now();
        if now >= until {
            return;
        }

        let dur = until - now;
        executor::timeout(dur, self.wait()).await.ok();
    }
}

/// After a maximum amount of time or maximum number of requests is received,
/// this will trigger a given function to execute whose result will be passed
/// along to every entry requesting a slot in the batch
pub struct Batch<F, R> {
    func: F,
    entries: Vec<oneshot::Sender<R>>,
    max_size: usize,
    expires: Instant,
}

impl<F: Fn() -> K, K: Future<Output = R>, R: Copy> Batch<F, R> {
    pub fn new(max_size: usize, expires: Instant, func: F) -> Self {
        Batch {
            func,
            entries: vec![],
            expires,
            max_size,
        }
    }

    pub fn push(&mut self) {
        if self.entries.len() == 0 {
            // Spin off a task that blocks for stuff
            // Basically one of our two conditions
            // Then likewise return the main one
        }
    }

    // What is interesting is that we could init it multiple times if we really
    // wanted to
}
