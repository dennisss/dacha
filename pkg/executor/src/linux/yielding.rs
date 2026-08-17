use core::future::Future;
use core::pin::Pin;
use core::task::Poll;

use base_error::*;

use crate::linux::task::Task;
use crate::linux::thread_local::CurrentTaskContext;

pub fn yield_now() -> impl Future<Output = Result<()>> {
    YieldFuture { done: false }
}

struct YieldFuture {
    done: bool
}

impl Future for YieldFuture {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<Result<()>> {
        let current_task = CurrentTaskContext::current().unwrap();

        if !self.done {
            self.done = true;
            let mut state = current_task.state.lock().unwrap();
            state.yielding = true;
            return Poll::Pending;
        }

        Poll::Ready(Ok(()))
    }
}