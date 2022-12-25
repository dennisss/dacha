// old code for an epoll based executor.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::sync::Arc;

use common::errors::*;
use sys::{EpollEvent, EpollEvents};

use crate::linux::executor::{Executor, ExecutorShared, FileDescriptor};

use super::waker::retrieve_task_entry;

struct ExecutorEpoll {
    poller: Epoll,

    /// Eventfd descriptor which is always polled for changes by the main
    /// thread.
    ///
    /// TODO: Close this on drop.
    polled_eventfd: FileDescriptor,

    /// Set of which file descriptors need to be polled and which tasks are
    /// requesting them.
    polled_descriptors: Mutex<HashMap<FileDescriptor, TaskId>>,
}
// poller: Epoll::new()?,
// polled_eventfd: unsafe { sys::eventfd2(0, sys::O_CLOEXEC | sys::O_NONBLOCK)
// }?, polled_descriptors: Mutex::new(HashMap::new()),

/// Registers that a file descriptor should be watched for some events to
/// occur. When any of the events is triggered, the given task will be woken
/// up.
pub(super) fn register_file_descriptor(
    shared: &Arc<ExecutorShared>,
    task_id: TaskId,
    fd: FileDescriptor,
    events: EpollEvents,
) -> Result<()> {
    let mut polled_descs = shared.polled_descriptors.lock().unwrap();
    if polled_descs.contains_key(&fd) {
        return Err(err_msg(
            "Only allowed to have more than one waiter for a file descriptor.",
        ));
    }

    let mut event = EpollEvent::default();
    event.set_fd(fd);
    event.set_events(events);
    shared.poller.control(EpollOp::EPOLL_CTL_ADD, fd, &event)?;

    polled_descs.insert(fd, task_id);

    Ok(())
}

pub(super) fn unregister_file_descriptor(shared: &ExecutorShared, fd: FileDescriptor) {
    let unused = EpollEvent::default(); // TODO: Can be a nullptr.
    let _ = shared.poller.control(EpollOp::EPOLL_CTL_DEL, fd, &unused);

    // TODO: If CTL_DEL fails, should we keep this?
    shared.polled_descriptors.lock().unwrap().remove(&fd);
}

/// Tells the main run() thread which is polling all file descriptors to
/// wake up (and re-generate the set of files to watch).
///
/// TODO: Deduplicate this code.
fn notify_polling_thread(shared: &ExecutorShared) -> Result<()> {
    // TODO: If this fails, should we remove the device from the list?
    let event_num: u64 = 1;
    let n = unsafe {
        sys::write(
            shared.polled_eventfd,
            core::mem::transmute(&event_num),
            core::mem::size_of::<u64>(),
        )
    };
    if n != Ok(core::mem::size_of::<u64>()) {
        return Err(err_msg("Failed to notify background thread"));
    }

    // TODO: Ignore EAGAIN errors. Mains that the counter overflowed (meaning that
    // it already has a value set.)

    Ok(())
}

///
pub(super) struct PollingContext {
    executor_shared: Arc<ExecutorShared>,
    fd: FileDescriptor,
}

impl PollingContext {
    pub fn create(fd: FileDescriptor, events: EpollEvents) -> impl Future<Output = Result<Self>> {
        CreatePollingContextFuture { fd, events }
    }
}

impl Drop for PollingContext {
    fn drop(&mut self) {
        Executor::unregister_file_descriptor(&self.executor_shared, self.fd);
    }
}

struct CreatePollingContextFuture {
    fd: FileDescriptor,
    events: EpollEvents,
}

impl CreatePollingContextFuture {
    fn poll_with_result(&self, context: &mut Context<'_>) -> Result<PollingContext> {
        let task_entry = retrieve_task_entry(context)
            .ok_or_else(|| err_msg("Not running inside an executor"))?;

        Executor::register_file_descriptor(
            &task_entry.executor_shared,
            task_entry.id,
            self.fd,
            self.events,
        )?;

        Ok(PollingContext {
            executor_shared: task_entry.executor_shared.clone(),
            fd: self.fd,
        })
    }
}

impl Future for CreatePollingContextFuture {
    type Output = Result<PollingContext>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.poll_with_result(context))
    }
}
