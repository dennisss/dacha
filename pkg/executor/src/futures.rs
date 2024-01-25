use core::future::Future;
use core::marker::PhantomData;
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

pub struct Optional<F> {
    future: Option<F>,
}

impl<T, F: Future<Output = T>> Future for Optional<F> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let future = unsafe { Pin::new_unchecked(&mut this.future) };

        if let Some(f) = &mut this.future {
            let future = unsafe { Pin::new_unchecked(f) };
            future.poll(cx)
        } else {
            Poll::Pending
        }
    }
}

pub fn optional<F>(future: Option<F>) -> Optional<F> {
    Optional { future }
}

pub struct Pending<T> {
    _data: PhantomData<T>,
}

impl<T> Future for Pending<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

pub fn pending<T>() -> Pending<T> {
    Pending { _data: PhantomData }
}
