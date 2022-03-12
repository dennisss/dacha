use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub struct Map<F, M> {
    future: F,
    mapper: M,
}

impl<T, Y, F: Future<Output = T>, M: Fn(T) -> Y> Future for Map<F, M> {
    type Output = Y;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let future = unsafe { Pin::new_unchecked(&mut this.future) };
        let mapper = &this.mapper;

        match future.poll(cx) {
            Poll::Ready(v) => Poll::Ready(mapper(v)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn map<F, M>(future: F, mapper: M) -> Map<F, M> {
    Map { future, mapper }
}
