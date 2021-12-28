use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub struct Race2<T, A: Future<Output = T>, B: Future<Output = T>> {
    a: A,
    b: B,
}

impl<T, A: Future<Output = T>, B: Future<Output = T>> Future for Race2<T, A, B> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        let this = unsafe { self.get_unchecked_mut() };
        let a = unsafe { Pin::new_unchecked(&mut this.a) };
        let b = unsafe { Pin::new_unchecked(&mut this.b) };

        if let Poll::Ready(v) = a.poll(_cx) {
            return Poll::Ready(v);
        }

        if let Poll::Ready(v) = b.poll(_cx) {
            return Poll::Ready(v);
        }

        return Poll::Pending;
    }
}

pub fn race2<T, A: Future<Output = T>, B: Future<Output = T>>(a: A, b: B) -> Race2<T, A, B> {
    Race2 { a, b }
}

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
