use common::async_std::future;
use common::async_std::sync::{Mutex, MutexGuard};
use common::async_std::task;
use common::futures::channel::mpsc;
use common::futures::channel::oneshot;
use common::futures::future::*;
use common::futures::prelude::Sink;
use common::futures::{Future, Stream};
use common::futures::{SinkExt, StreamExt};
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};
use std::time::Instant;

/// Pretty much a futures based implementation of a conditional variable that
/// owns the condition value
/// Unlike a conditional variable, this will not relock the mutex after the wait
/// is done
///
/// NOTE: It should not be locked for a long period of time as that is still a
/// blocking operation
/// We also allow listeners to store a small value when they call wait()
/// A notifier can optionally read this value to filter exactly which waiters
/// are woken up
pub struct Condvar<V, T = ()> {
    inner: Mutex<CondvarInner<V, T>>,
}

struct CondvarInner<V, T> {
    value: V,
    waiters: Vec<(oneshot::Sender<()>, T)>,
}

impl<V, T> CondvarInner<V, T> {
    /// Garbage collects all waiters which are no longer being waited on
    fn collect(&mut self) {
        let mut i = 0;
        while i < self.waiters.len() {
            let dropped = self.waiters[i].0.is_canceled();

            if dropped {
                self.waiters.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
}

impl<V, T> Condvar<V, T> {
    // TODO: It would be most reasonable to give the comparator function up
    // front or implement it upfront as a trait upfront so that the notifier
    // doesn't have to worry about passing in a tester
    pub fn new(initial_value: V) -> Self {
        Condvar {
            // TODO: Implement a a lock free list + Atomic variable instead?
            inner: Mutex::new(CondvarInner {
                value: initial_value,
                waiters: vec![],
            }),
        }
    }

    pub async fn lock<'a>(&'a self) -> CondvarGuard<'a, V, T> {
        CondvarGuard {
            guard: self.inner.lock().await,
        }
    }
}

pub struct CondvarGuard<'a, V, T> {
    guard: MutexGuard<'a, CondvarInner<V, T>>,
}

impl<'a, V, T> Borrow<V> for CondvarGuard<'a, V, T> {
    fn borrow(&self) -> &V {
        &self.guard.value
    }
}

impl<'a, V, T> BorrowMut<V> for CondvarGuard<'a, V, T> {
    fn borrow_mut(&mut self) -> &mut V {
        &mut self.guard.value
    }
}

impl<'a, V, T> Deref for CondvarGuard<'a, V, T> {
    type Target = V;
    fn deref(&self) -> &V {
        &self.guard.value
    }
}

impl<'a, V, T> DerefMut for CondvarGuard<'a, V, T> {
    fn deref_mut(&mut self) -> &mut V {
        &mut self.guard.value
    }
}

impl<'a, V, T> CondvarGuard<'a, V, T> {
    pub async fn wait(self, data: T) {
        // LockResult<MutexGuard<'a, T>> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.guard;

        // TODO: Currently no mechanism for effeciently cleaning up waiters
        // without having to look through all of them
        guard.collect();

        guard.waiters.push((tx, data));

        // NOTE: This will be dropped anyway as soon as the future is returned
        drop(guard);

        rx.await.ok(); // TODO: Check this.
    }

    // TODO: Should we immediately consume and drop the guard
    pub fn notify_filter<F>(&mut self, f: F)
    where
        F: Fn(&T) -> bool,
    {
        let guard = &mut self.guard;

        let mut i = guard.waiters.len();
        while i > 0 {
            let notify = f(&guard.waiters[i - 1].1);
            if notify {
                let (tx, _) = guard.waiters.swap_remove(i - 1);
                if let Err(_) = tx.send(()) {
                    // In this case, the waiter was deallocated and doesn't
                    // matter anymore
                    // TODO: I don't think the oneshot channel emits any real
                    // errors though and should always succeed if not
                    // deallocated?
                }
            }

            i -= 1;
        }
    }

    pub fn notify_all(&mut self) {
        self.notify_filter(|_| true);
    }
}

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
    let (tx, rx) = mpsc::channel(0);
    let tx2 = tx.clone();

    (ChangeSender(tx), ChangeReceiver(tx2, rx))
}

pub struct ChangeSender(mpsc::Sender<()>);

impl ChangeSender {
    // TODO: In general we shouldn't use this as we are now mostly operating in a
    // threaded environment
    pub fn notify(&mut self) {
        if let Err(e) = self.0.try_send(()) {
            // This will fail in one of two cases:
            // 1. Either the channel is full (which is fine as we only want a
            // single notification to get scheduled at a time)
            // 2. The other side of disconnected (will be handled by some other
            // code)
        }
    }
}

pub struct ChangeReceiver(mpsc::Sender<()>, mpsc::Receiver<()>);

impl ChangeReceiver {
    /// Waits indefinately until the change occurs for the first time.
    pub async fn wait(self) -> ChangeReceiver {
        let (sender, receiver) = (self.0, self.1);
        let (_, receiver) = receiver.into_future().await;
        Self(sender, receiver)

        //		let waiter = receiver.into_future().then(|res| -> FutureResult<_, ()>
        // { 			match res {
        //				Ok((_, receiver)) => ok(receiver),
        //				Err((_, receiver)) => ok(receiver)
        //			}
        //		});
        //
        //		waiter
        //		.and_then(|receiver| {
        //			ok(ChangeReceiver(sender, receiver))
        //		})
    }

    pub async fn wait_until(self, until: Instant) -> ChangeReceiver {
        let now = Instant::now();
        if now <= until {
            return self;
        }

        let mut delay_sender = self.0.clone();
        // TODO: Ideally 'select' between this and the other one so that the
        // timer can be cleaned up before
        task::spawn(async move {
            common::wait_for(now - until).await;
            delay_sender.send(()).await
            // Return a pending future so that this never resolves?
        });

        self.wait().await

        // TODO:
        /*
        common::async_std::future::timeout()

        let delay = tokio::timer::Delay::new(until)
        .then(move |_| {
            delay_sender.send(())
        })
        .then(|_| -> Empty<_, ()> {
            empty()
        });

        self.wait()
        .select(delay)
        // In general nothing should error out with this (only the timer may
        error out, but the delay_sender should never error out in a reasonable
        way as we have our own dedicated copy of it)
        .map_err(|_| ())
        .map(|(c, _)| {
            c
        })
        */
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
