use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::Thread;

use common::errors::*;
use common::hash::FastHasherBuilder;
use common::io::{IoError, IoErrorKind};

use crate::linux::executor::{ExecutorShared, TaskId};
use crate::linux::waker::retrieve_task_entry;

use super::task::TaskEntry;
use super::thread_local::{CurrentTaskContext, CurrentExecutorContext};

/// Reserved token that is used to force the polling thread to wake up
/// (mainly so that it can notice when we are shutting down)
const WAKE_TOKEN: mio::Token = mio::Token(0);

pub(super) struct ExecutorMio {
    registry: mio::Registry,
    submit_state: Mutex<SubmitState>,
    poll_state: Mutex<PollState>,
    
    /// This increments each time we complete a poll.
    poll_epoch: AtomicU64,
}

#[derive(Default)]
struct SubmitState {
    last_token_id: usize,
    polled_tokens: HashMap<mio::Token, TokenState, FastHasherBuilder>,
    shutting_down: bool,
}

#[derive(Default)]
struct TokenState {
    /// This advances to 'ExecutorMio::poll_epoch' when we get events for
    /// this token.
    epoch: u64,

    task_ids: Vec<u64>,
}

struct PollState {
    poll: mio::Poll,
    events: mio::Events,
}

impl ExecutorMio {
    pub fn create() -> Result<Self> {

        let mut poll = mio::Poll::new()?;
        let registry = poll.registry().try_clone().unwrap();

        let mut events = mio::Events::with_capacity(128);

        Ok(Self {
            registry,
            submit_state: Default::default(),
            poll_state: Mutex::new(PollState {
                poll,
                events
            }),
            poll_epoch: Default::default()
        })
    }

    /// Waits until at least one operation is complete and retrieves the set of
    /// tasks that need to be woken up.
    ///
    /// NOTE: We strictly append to 'tasks_to_wake'.
    pub fn poll_events(
        &self,
        tasks_to_wake: &mut HashSet<TaskId, FastHasherBuilder>,
    ) -> Result<()> {

        let mut poll_state = self.poll_state.lock().unwrap();
        let poll_state = &mut *poll_state;
        
        poll_state.poll.poll(&mut poll_state.events, None)?;

        let epoch = self.poll_epoch.fetch_add(1, Ordering::SeqCst) + 1;

        let mut submit_state = self.submit_state.lock().unwrap();
        for event in poll_state.events.iter() {

            let token_state = match submit_state.polled_tokens.get_mut(&event.token()) {
                Some(v) => v,
                None => continue
            };

            token_state.epoch = epoch;
            tasks_to_wake.extend(token_state.task_ids.drain(..));
        }

        Ok(())
    }

    /// This can be used to determine when to stop calling poll_events().
    pub fn finished(&self) -> bool {
        self.submit_state.lock().unwrap().shutting_down
    }

    /// Triggers any callers to poll_events() to unblock shortly after this is
    /// called.
    pub fn wake_poller(&self) -> Result<()> {
        mio::Waker::new(&self.registry, WAKE_TOKEN).unwrap().wake().unwrap();
        Ok(())
    }

    pub fn shutdown(&self) {
        self.submit_state.lock().unwrap().shutting_down = true;

        // Wake any pollers waiting for operations to appear/complete.
        self.wake_poller().unwrap();
    }
}

/// Wrapper for a mio source which ensures that it is registered in the poller
/// registry.
pub struct ExecutorMioSource<S: mio::event::Source> {
    executor: Arc<ExecutorShared>,
    // TODO: Use one that poisons?
    source: Mutex<S>,
    token: mio::Token,
}

impl<S: mio::event::Source> ExecutorMioSource<S> {
    pub fn create(mut source: S) -> Result<Self> {
        let executor = CurrentExecutorContext::current().unwrap();
        let inst = &executor.mio;

        let token = {
            let mut submit_state = inst.submit_state.lock().unwrap();
            let id = submit_state.last_token_id + 1;
            submit_state.last_token_id = id;
            mio::Token(id)
        };

        inst.registry.register(
            &mut source,
            token,
            // TODO: Maybe optimize this.
            mio::Interest::READABLE | mio::Interest::WRITABLE
        )?;

        Ok(Self {
            executor,
            source: Mutex::new(source),
            token
        })
    }

    pub async fn retry_blocking<T, F: FnMut(&mut S) -> Result<T, std::io::Error>>(&self, mut f: F) -> Result<T, std::io::Error> {
        // TODO: Limit max iterations
        loop {
            let waiter = self.waiter();

            let res = {
                let mut s = self.source.lock().unwrap();
                f(&mut *s)
            };
            match res {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        waiter.await;
                        continue;
                    }

                    return Err(e.into());
                }
            }
        }
    }

    pub fn run<T, F: Fn(&mut S) -> T>(&self, mut f: F) -> T {
        let mut s = self.source.lock().unwrap();
        f(&mut s)
    }

    fn waiter<'a>(&'a self) -> ExecutorMioWaiter<'a, S> {
        let inst = &self.executor.mio;

        ExecutorMioWaiter {
            inst: self,
            last_epoch: inst.poll_epoch.load(Ordering::SeqCst),
        }
    }
}

impl<S: mio::event::Source> Drop for ExecutorMioSource<S> {
    fn drop(&mut self) {
        let mut s = self.source.lock().unwrap();

        // TODO: Log error.
        let _ = self.executor.mio.registry.deregister(&mut *s);

        self.executor.mio.submit_state.lock().unwrap().polled_tokens.remove(&self.token);
    }
}

pub struct ExecutorMioWaiter<'a, S: mio::event::Source> {
    inst: &'a ExecutorMioSource<S>,
    last_epoch: u64
}

impl<'a, S: mio::event::Source> Future for ExecutorMioWaiter<'a, S> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let task_entry = match retrieve_task_entry(context) {
            Some(v) => v,
            None => return Poll::Ready(Err(err_msg("Not running inside an executor"))),
        };

        let this = unsafe { self.get_unchecked_mut() };

        let executor = &task_entry.executor_shared;
        let inst = &executor.mio;

        let mut submit_state = inst.submit_state.lock().unwrap();

        if submit_state.shutting_down {
            return Poll::Ready(Err(IoError::new(
                IoErrorKind::Cancelled,
                "Executor shutting down",
            )
            .into()));
        }

        let token_state = submit_state
            .polled_tokens.entry(this.inst.token).or_default();

        if token_state.epoch > this.last_epoch {            
            return Poll::Ready(Ok(()));
        }

        if !token_state.task_ids.contains(&task_entry.id) {
            token_state.task_ids.push(task_entry.id);
        }

        Poll::Pending
    }
}
