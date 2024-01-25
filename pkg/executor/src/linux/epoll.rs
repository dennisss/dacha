use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use base_error::*;
use common::hash::FastHasherBuilder;
use common::io::{IoError, IoErrorKind};
use sys::{Epoll, EpollEvent, EpollEvents, EpollOp, OpenFileDescriptor};

use crate::linux::executor::{Executor, ExecutorShared, FileDescriptor, TaskId};
use crate::linux::thread_local::CurrentTaskContext;
use crate::linux::waker::retrieve_task_entry;

/// NOTE: Because we allow for polling to be requested on a different thread
/// than the one that is running the epoll loop, we require that all events are
/// event triggered.
pub(super) struct ExecutorEpoll {
    poller: Epoll,

    /// Eventfd descriptor which is always polled for changes by the main
    /// thread.
    polled_eventfd: OpenFileDescriptor,

    state: Mutex<State>,
}

struct State {
    running: bool,

    /// Set of which file descriptors need to be polled.
    polled_descriptors: HashMap<FileDescriptor, DescriptorState>,
}

struct DescriptorState {
    /// Task which is waiting for changes to this descriptor.
    task_id: Option<TaskId>,

    /// Events which have raised by the kernel but not yet processed by the
    /// waiting task.
    ///
    /// EpollEvents::empty() implies there is nothing new to process.
    events: EpollEvents,
}

impl ExecutorEpoll {
    pub fn create() -> Result<Self> {
        let polled_eventfd =
            OpenFileDescriptor::new(unsafe { sys::eventfd2(0, sys::O_CLOEXEC | sys::O_NONBLOCK) }?);

        let poller = Epoll::new()?;

        let mut e = EpollEvent::default();
        e.set_fd(*polled_eventfd);
        e.set_events(EpollEvents::EPOLLIN);
        poller.control(EpollOp::EPOLL_CTL_ADD, *polled_eventfd, &e)?;

        Ok(Self {
            poller,
            polled_eventfd,
            state: Mutex::new(State {
                running: true,
                polled_descriptors: HashMap::new(),
            }),
        })
    }

    pub fn poll_events(
        &self,
        tasks_to_wake: &mut HashSet<TaskId, FastHasherBuilder>,
    ) -> Result<()> {
        // TODO: Re-use this memory.
        let mut events = [EpollEvent::default(); 8];

        let nevents = self.poller.wait(&mut events)?;

        let mut state = self.state.lock().unwrap();

        for event in &events[0..nevents] {
            if event.fd() == *self.polled_eventfd {
                continue;
            }

            let desc_state = state
                .polled_descriptors
                .get_mut(&event.fd())
                .ok_or_else(|| {
                    format_err!(
                        "Unregistered fd: {}, events: {:?}",
                        event.fd(),
                        event.events()
                    )
                })?;

            desc_state.events |= event.events();

            if let Some(task_id) = desc_state.task_id.take() {
                tasks_to_wake.insert(task_id);
            }
        }

        Ok(())
    }

    pub fn shutdown(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.running = false;

            // TODO: Cancel/wakeup all pending tasks.
        }

        self.wake_poller().unwrap();
    }

    pub fn finished(&self) -> bool {
        !self.state.lock().unwrap().running
    }

    fn wake_poller(&self) -> Result<()> {
        let event_num: u64 = 1;
        let n = unsafe {
            sys::write(
                *self.polled_eventfd,
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

    /// Registers that a file descriptor should be watched for some events to
    /// occur. When any of the events is triggered, the given task will be woken
    /// up.
    fn register_file_descriptor(
        shared: &Arc<ExecutorShared>,
        fd: FileDescriptor,
        events: EpollEvents,
    ) -> Result<()> {
        let this = &shared.epoll;
        let mut state = this.state.lock().unwrap();

        if !state.running {
            return Err(IoError::new(IoErrorKind::Cancelled, "Polling is shutdown").into());
        }

        if state.polled_descriptors.contains_key(&fd) {
            return Err(err_msg(
                "Only allowed to have one waiter for a file descriptor.",
            ));
        }

        let mut event = EpollEvent::default();
        event.set_fd(fd);
        event.set_events(events | EpollEvents::EPOLLET);
        this.poller.control(EpollOp::EPOLL_CTL_ADD, fd, &event)?;

        state.polled_descriptors.insert(
            fd,
            DescriptorState {
                task_id: None,
                events: EpollEvents::empty(),
            },
        );

        Ok(())
    }

    /// NOTE: This assumes that there are no tasks left waiting for the file.
    fn unregister_file_descriptor(&self, fd: FileDescriptor) {
        let unused = EpollEvent::default(); // TODO: Can be a nullptr.
        if let Err(e) = self.poller.control(EpollOp::EPOLL_CTL_DEL, fd, &unused) {
            eprintln!("EPOLL_CTL_DEL failed: {}", e);
        }

        // TODO: If CTL_DEL fails, should we keep this?
        self.state.lock().unwrap().polled_descriptors.remove(&fd);
    }
}

pub struct ExecutorPollingContext<'a> {
    executor_shared: Arc<ExecutorShared>,
    fd: FileDescriptor,
    fd_lifetime: PhantomData<&'a ()>,
}

impl<'a> Drop for ExecutorPollingContext<'a> {
    fn drop(&mut self) {
        let current_task = CurrentTaskContext::current().unwrap();
        current_task
            .executor_shared
            .epoll
            .unregister_file_descriptor(self.fd);
    }
}

impl<'a> ExecutorPollingContext<'a> {
    pub fn create(
        file: &'a OpenFileDescriptor,
        events: EpollEvents,
    ) -> impl Future<Output = Result<ExecutorPollingContext<'a>>> {
        CreateExecutorPollingContextFuture {
            fd: **file,
            fd_lifetime: PhantomData,
            events,
        }
    }

    /// NOTE: Only safe if the caller ensures that the file outlives the
    /// context.
    pub unsafe fn create_with_raw_fd(
        fd: FileDescriptor,
        events: EpollEvents,
    ) -> impl Future<Output = Result<ExecutorPollingContext<'static>>> {
        CreateExecutorPollingContextFuture {
            fd,
            fd_lifetime: PhantomData,
            events,
        }
    }

    /// NOTE: Requires mutability as we only support one waiting task at a time.
    pub fn wait<'b>(&'b mut self) -> impl Future<Output = Result<EpollEvents>> + 'b {
        ExecutorPollingContextWaitFuture { context: self }
    }
}

struct CreateExecutorPollingContextFuture<'a> {
    fd: FileDescriptor,
    fd_lifetime: PhantomData<&'a ()>,
    events: EpollEvents,
}

impl<'a> CreateExecutorPollingContextFuture<'a> {
    fn poll_with_result(&self, context: &mut Context<'_>) -> Result<ExecutorPollingContext<'a>> {
        let task_entry = retrieve_task_entry(context)
            .ok_or_else(|| err_msg("Not running inside an executor"))?;

        ExecutorEpoll::register_file_descriptor(&task_entry.executor_shared, self.fd, self.events)?;

        Ok(ExecutorPollingContext {
            executor_shared: task_entry.executor_shared.clone(),
            fd: self.fd,
            fd_lifetime: self.fd_lifetime,
        })
    }
}

impl<'a> Future for CreateExecutorPollingContextFuture<'a> {
    type Output = Result<ExecutorPollingContext<'a>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.poll_with_result(context))
    }
}

// TODO: On drop, remove the task entry.
struct ExecutorPollingContextWaitFuture<'a, 'b> {
    context: &'a ExecutorPollingContext<'b>,
}

impl<'a, 'b> Future for ExecutorPollingContextWaitFuture<'a, 'b> {
    type Output = Result<EpollEvents>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let task_entry = match retrieve_task_entry(cx) {
            Some(v) => v,
            None => return Poll::Ready(Err(err_msg("Not running inside an executor"))),
        };

        let fd = self.context.fd;

        let mut state = self.context.executor_shared.epoll.state.lock().unwrap();

        let state = match state.polled_descriptors.get_mut(&fd) {
            Some(v) => v,
            None => {
                return Poll::Ready(Err(IoError::new(
                    IoErrorKind::Cancelled,
                    "Polling was cancelled",
                )
                .into()));
            }
        };

        if state.events != EpollEvents::empty() {
            let e = state.events;
            state.events = EpollEvents::empty();
            return Poll::Ready(Ok(e));
        }

        state.task_id = Some(task_entry.id);

        Poll::Pending
    }
}
