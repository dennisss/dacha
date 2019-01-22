use std::time::{Duration, Instant};
use std::sync::{Mutex, MutexGuard};
use futures::future::*;
use futures::{Future, Stream};
use futures::sync::mpsc;
use futures::prelude::Sink;
use futures::sync::oneshot;
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};


/// Pretty much a futures based implementation of a conditional variable that owned the condition value
/// Unlike a conditional variable, this will not relock the mutex after the wait is done
/// 
/// NOTE: It should still not be locked for a long period of time as that is still a blocking operation
/// We also allow listeners to store a small value when they call wait()
/// A notifier can optionally read this value to filter exactly which waiters are woken up
pub struct Condition<V, T = ()> {
	inner: Mutex<ConditionInner<V, T>>
}

struct ConditionInner<V, T> {
	value: V,
	waiters: Vec<(oneshot::Sender<()>, T)> 
}

impl<V, T> ConditionInner<V, T> {
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


impl<V, T> Condition<V, T> {
	pub fn new(initial_value: V) -> Self {
		Condition {
			inner: Mutex::new(ConditionInner {
				value: initial_value,
				waiters: vec![]
			})
		}
	}

	pub fn lock<'a>(&'a self) -> ConditionGuard<'a, V, T> {
		ConditionGuard {
			guard: self.inner.lock().unwrap()
		}
	}
}

pub struct ConditionGuard<'a, V, T> {
	guard: MutexGuard<'a, ConditionInner<V, T>>
}

impl<'a, V, T> Borrow<V> for ConditionGuard<'a, V, T> {
	fn borrow(&self) -> &V {
		&self.guard.value
	}
}

impl<'a, V, T> BorrowMut<V> for ConditionGuard<'a, V, T> {
	fn borrow_mut(&mut self) -> &mut V {
		&mut self.guard.value
	}
}

impl<'a, V, T> Deref for ConditionGuard<'a, V, T> {
	type Target = V;
	fn deref(&self) -> &V {
		&self.guard.value
	}
}

impl<'a, V, T> DerefMut for ConditionGuard<'a, V, T> {
	fn deref_mut(&mut self) -> &mut V {
		&mut self.guard.value
	}
}



impl<'a, V, T> ConditionGuard<'a, V, T> {
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
				tx.send(());
			}

			i -= 1;
		}

		/*
		let mut i = 0;
		while i < guard.waiters.len() {
			let notify = f(&guard.waiters[i].1);
			if notify {
				let (tx, v) = guard.waiters.swap_remove(i);
				tx.send(());
			}
			else {
				i += 1;
			}
		}
		*/
	}

	pub fn notify_all(&mut self) {
		self.notify_filter(|_| true);
	}

}




/// Creates a futures based event notification channel
/// 
/// In other words this is an mpsc with the addition of being able to wait for the event with a maximum timeout and being able to send without consuming the sender
/// Useful for allowing one receiver to get notified of checking occuring anywhere else in the system
pub fn event() -> (EventSender, EventReceiver) {
	let (tx, rx) = mpsc::channel(0);
	let tx2 = tx.clone();

	(
		EventSender(tx),
		EventReceiver(tx2, rx)
	)
}

pub struct EventSender(mpsc::Sender<()>);

impl EventSender {
	pub fn notify(&self) {
		// TODO: Can we make this more efficient
		tokio::spawn(
			self.0.clone().send(()).map(|_| ()).map_err(|_| ())
		);
	}
}


pub struct EventReceiver(mpsc::Sender<()>, mpsc::Receiver<()>);

impl EventReceiver {

	// TODO: Probably need to eventually handle errors occuring in this in some reasonable way
	pub fn wait(self, dur: Duration) -> impl Future<Item=EventReceiver, Error=()> + Send {

		let (sender, receiver) = (self.0, self.1);

		// TODO: Maybe take it as an input
		let until = Instant::now() + dur;

		let delay_sender = sender.clone();

		// Simpler question is can we make this stop early
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
		.and_then(|(receiver, _): (mpsc::Receiver<()>, _)| -> FutureResult<EventReceiver, _> {
			ok(EventReceiver(sender, receiver))
		})
		
	}
}

