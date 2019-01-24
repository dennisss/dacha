use std::time::{Instant};
use std::sync::{Mutex, MutexGuard, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use futures::future::*;
use futures::{Future, Stream};
use futures::sync::mpsc;
use futures::prelude::Sink;
use futures::sync::oneshot;
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};


/// Pretty much a futures based implementation of a conditional variable that owns the condition value
/// Unlike a conditional variable, this will not relock the mutex after the wait is done
/// 
/// NOTE: It should not be locked for a long period of time as that is still a blocking operation
/// We also allow listeners to store a small value when they call wait()
/// A notifier can optionally read this value to filter exactly which waiters are woken up
pub struct Condvar<V, T = ()> {
	inner: Mutex<CondvarInner<V, T>>
}

struct CondvarInner<V, T> {
	value: V,
	waiters: Vec<(oneshot::Sender<()>, T)> 
}

impl<V, T> CondvarInner<V, T> {
	/// Garbage collects all waiters which are no longer being waited on
	fn collect(&mut self) {
		let mut i = 0;
		while i < self.waiters.len() {
			let dropped = match self.waiters[i].0.poll_cancel() {
				Ok(futures::Async::Ready(_)) => true,
				_ => false
			};
			
			if dropped {
				self.waiters.swap_remove(i);
			}
			else {
				i += 1;
			}
		}
	}
}

impl<V, T> Condvar<V, T> {
	// TODO: It would be most reasonable to give the comparator function up front or implement it upfront as a trait upfront so that the notifier doesn't have to worry about passing in a tester
	pub fn new(initial_value: V) -> Self {
		Condvar {
			// TODO: Implement a a lock free list + Atomic variable instead?
			inner: Mutex::new(CondvarInner {
				value: initial_value,
				waiters: vec![]
			})
		}
	}

	pub fn lock<'a>(&'a self) -> CondvarGuard<'a, V, T> {
		CondvarGuard {
			guard: self.inner.lock().unwrap()
		}
	}
}

pub struct CondvarGuard<'a, V, T> {
	guard: MutexGuard<'a, CondvarInner<V, T>>
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
	pub fn wait(self, data: T) -> impl Future<Item=(), Error=()> + Send {  // LockResult<MutexGuard<'a, T>> {
		let (tx, rx) = oneshot::channel();
		let mut guard = self.guard;

		// TODO: Currently no mechanism for effeciently cleaning up waiters without having to look through all of them
		guard.collect();

		guard.waiters.push((tx, data));
		
		// NOTE: This will be dropped anyway as soon as the future is returned
		drop(guard);

		rx.then(|_| {
			ok(())
		})
	}

	// TODO: Should we immediatley consume and drop the guard
	pub fn notify_filter<F>(&mut self, f: F) where F: Fn(&T) -> bool {
		let guard = &mut self.guard;

		let mut i = guard.waiters.len();
		while i > 0 {
			let notify = f(&guard.waiters[i - 1].1);
			if notify {
				let (tx, _) = guard.waiters.swap_remove(i - 1);
				if let Err(_) = tx.send(()) {
					// In this case, the waiter was deallocated and doesn't matter anymore
					// TODO: I don't think the oneshot channel emits any real errors though and should always succeed if not deallocated?
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
/// In other words this is an mpsc with the addition of being able to wait for the event with a maximum timeout and being able to send without consuming the sender
/// Useful for allowing one receiver to get notified of checking occuring anywhere else in the system
/// NOTE: This is a non-data carrying and non-event counting channel that functions pretty much just as a dirty flag which will batch all notifications into one for the next time the waiter is woken up. 
/// We use an atomic boolean along side an mpsc to deduplicate multiple sequential notifications occuring before the waiter gets woken up
pub fn change() -> (ChangeSender, ChangeReceiver) {
	let (tx, rx) = mpsc::channel(0);
	let tx2 = tx.clone();

	let dirty = Arc::new(AtomicBool::new(false));

	(
		ChangeSender(dirty.clone(), tx),
		ChangeReceiver(dirty, tx2, rx)
	)
}

pub struct ChangeSender(Arc<AtomicBool>, mpsc::Sender<()>);

impl ChangeSender {
	// TODO: In general we shouldn't use this as we are now mostly operating in a threaded environment
	pub fn notify(&self) {		
		// Allowing only the first notification to set the variable to true from false to notify the task
		if !self.0.swap(true, Ordering::SeqCst) {
			self.1.clone().try_send(());
		}
	}
}


pub struct ChangeReceiver(Arc<AtomicBool>, mpsc::Sender<()>, mpsc::Receiver<()>);

impl ChangeReceiver {

	// TODO: Probably need to eventaully handle errors occuring in this in some reasonable way
	// TODO: Would also be good to have a more efficient version which does not require setting up a timer
	pub fn wait(self, until: Instant) -> impl Future<Item=ChangeReceiver, Error=()> + Send {

		let (dirty, sender, receiver) = (self.0, self.1, self.2);

		let delay_sender = sender.clone();

		// TODO: In the case that we don't need to block for anything, then this is unnecessarily complex
		let delay = tokio::timer::Delay::new(until)
		.then(move |_| {
			delay_sender.send(())
		})
		.then(|_| -> Empty<_, ()> {
			empty()
		});

		let waiter = receiver.into_future().then(|res| -> FutureResult<_, ()> {
			match res {
				Ok((_, receiver)) => {
					ok(receiver)
				},
				Err((_, receiver)) => {
					ok(receiver)
				}
			}
		});

		waiter
		.select(delay)
		.map_err(|_| ())
		.and_then(|(receiver, _): (mpsc::Receiver<()>, _)| -> FutureResult<ChangeReceiver, _> {
			dirty.store(false, Ordering::SeqCst);
			ok(ChangeReceiver(dirty, sender, receiver))
		})
		
	}
}

