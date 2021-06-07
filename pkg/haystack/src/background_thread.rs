
use std::{thread, time};
use std::sync::{Mutex,Condvar};
use std::sync::atomic::{AtomicBool, Ordering};

/// Helper struct that wraps a start/stoppable thread that blocks for external events to occur
pub struct BackgroundThread {
	running: AtomicBool,
	handle: Mutex<Option<thread::JoinHandle<()>>>,

	/// Notifies the heartbeat/directory-sync thread whenever one of the following events occurs:
	/// 1. Server is shutting down
	/// 2. Volume has become read-only
	/// 3. Amount of total space has changed (usually we will just restart the store?)
	/// 4. Volume has been created/deleted (for the case of a change in allocation amount in this machine)
	event_var: Condvar,
	event_mutex: Mutex<bool>, // TODO: We might as well use this variable as the running value 

}

impl BackgroundThread {

	/// Creates a new not yet started background thread instance
	pub fn new() -> BackgroundThread {
		BackgroundThread {
			running: AtomicBool::new(false),
			handle: Mutex::new(None),
			event_var: Condvar::new(),
			event_mutex: Mutex::new(false),
		} 
	}

	/// NOTE: This is not safe to call more than once
	pub fn start<F>(&self, f: F) where F: FnOnce(), F: Send + 'static {
		self.running.store(true, Ordering::SeqCst);
		self.handle.lock().unwrap().replace(thread::spawn(f));
	}

	pub fn stop(&self) {
		self.running.store(false, Ordering::SeqCst);
		self.notify();

		// Block until thread is done
		let thread = self.handle.lock().unwrap().take().unwrap();
		thread.join().expect("Background thread panicked!");
	}

	pub fn is_running(&self) -> bool {
		self.running.load(Ordering::SeqCst)
	}

	pub async fn notify(&self) {
		let mut guard = self.event_mutex.lock().await;
		*guard = true;

		self.event_var.notify_one();
	}

	/// Should be called within the thread function to wait for the next event to occur or a timeout to elapse
	pub async fn wait(&self, time: u64) {
		let dur = time::Duration::from_millis(time);

		let mut guard = self.event_mutex.lock().await;
		if *guard {
			*guard = false;
			println!("Processing existing event");
		}
		else {
			let (mut next_guard, r) = self.event_var
				.wait_timeout(guard, dur).unwrap();
			
			*next_guard = false;

			if !r.timed_out() {
				println!("Sync thread got event!");
			}
		}
	}
}