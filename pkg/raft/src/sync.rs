use std::time::{Duration, Instant};
use futures::future::*;
use futures::{Future, Stream};
use futures::sync::mpsc;
use futures::prelude::Sink;



/// Creates a futures based event channel that acts like a Condvar
/// - The receiver can block until it is notified (or a timeout has elapsed)
/// - The sender can wakeup the pending receiver
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

