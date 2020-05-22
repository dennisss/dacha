use std::cell::Cell;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

pub struct TaskBundle<'a> {
    active: Arc<RwLock<bool>>,
    handles: Vec<async_std::task::JoinHandle<()>>,
    scope: PhantomData<&'a ()>,
}

impl<'a> TaskBundle<'a> {
    pub fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(true)),
            handles: vec![],
            scope: PhantomData,
        }
    }

    pub fn add<'b, F: Future<Output = ()> + Send + 'b>(&mut self, f: F)
    where
        'a: 'b,
    {
        let fboxed: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(f);
        let fstatic: Pin<Box<dyn Future<Output = ()> + Send + 'static>> =
            unsafe { std::mem::transmute(fboxed) };
        self.handles.push(async_std::task::spawn(TaskFuture {
            active: self.active.clone(),
            fut: fstatic,
        }));
    }

    pub async fn join(mut self) {
        for handle in &mut self.handles {
            handle.await;
        }
    }

    // TODO: Enable cancelling cancelable futures.
}

impl<'a> Drop for TaskBundle<'a> {
    // If the bundle is dropped before all tasks after completed, it will block
    // until they are all done.
    fn drop(&mut self) {
        *self.active.write().unwrap() = false;
    }
}

struct TaskFuture {
    active: Arc<RwLock<bool>>,
    fut: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
}

impl Future for TaskFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Must hold a reader lock shared with the main bundle to run.
        let active = self.active.clone();
        let active_guard = active.read().unwrap();
        if !*active_guard {
            return Poll::Ready(());
        }

        self.fut.as_mut().poll(cx)
    }
}
