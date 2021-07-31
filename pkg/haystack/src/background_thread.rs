use std::sync::atomic::{AtomicBool, Ordering};
use std::time;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;

/// Helper struct that wraps a start/stoppable thread that blocks for external
/// events to occur
pub struct BackgroundThread {
    running: AtomicBool,

    // TODO: Switch to using a ChildHandle.
    handle: Mutex<Option<task::JoinHandle<()>>>,

    /// Notifies the heartbeat/directory-sync thread whenever one of the
    /// following events occurs: 1. Server is shutting down
    /// 2. Volume has become read-only
    /// 3. Amount of total space has changed (usually we will just restart the
    /// store?) 4. Volume has been created/deleted (for the case of a change
    /// in allocation amount in this machine)
    event_sender: channel::Sender<()>,
    event_receiver: channel::Receiver<()>,
}

impl BackgroundThread {
    /// Creates a new not yet started background thread instance
    pub fn new() -> BackgroundThread {
        let (event_sender, event_receiver) = channel::bounded(1);

        BackgroundThread {
            running: AtomicBool::new(false),
            handle: Mutex::new(None),
            event_sender,
            event_receiver,
        }
    }

    /// NOTE: This is not safe to call more than once
    pub async fn start<Fut: 'static + std::future::Future<Output = ()> + Send>(&self, future: Fut) {
        self.running.store(true, Ordering::SeqCst);
        self.handle.lock().await.replace(task::spawn(future));
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.notify();

        // Block until thread is done
        let thread = self.handle.lock().await.take().unwrap();

        thread.await;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn notify(&self) {
        let _ = self.event_sender.try_send(());
    }

    /// Should be called within the thread function to wait for the next event
    /// to occur or a timeout to elapse
    pub async fn wait(&self, time: u64) {
        let dur = time::Duration::from_millis(time);

        let timeout_sender = self.event_sender.clone();
        let timeout = task::spawn(async move {
            task::sleep(dur).await;
            let _ = timeout_sender.try_send(());
        });

        let _ = self.event_receiver.recv().await;

        timeout.cancel().await;

        // let mut guard = self.event_mutex.lock().await;
        // if *guard {
        // 	*guard = false;
        // 	println!("Processing existing event");
        // }
        // else {
        // 	let (mut next_guard, r) = self.event_var
        // 		.wait_timeout(guard, dur).unwrap();

        // 	*next_guard = false;

        // 	if !r.timed_out() {
        // 		println!("Sync thread got event!");
        // 	}
        // }
    }
}
