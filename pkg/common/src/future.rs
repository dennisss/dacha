use std::future::Future;

use std::pin::Pin;
use std::task::Poll;

pub struct Race<F1, F2> {
    f1: Pin<Box<F1>>,
    f2: Pin<Box<F2>>,
}

pub fn race<F1, F2>(f1: F1, f2: F2) -> Race<F1, F2> {
    Race {
        f1: Box::pin(f1),
        f2: Box::pin(f2),
    }
}

impl<T, F1: Future<Output = T>, F2: Future<Output = T>> Future for Race<F1, F2> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match self.f1.as_mut().poll(cx) {
            Poll::Ready(v) => {
                return Poll::Ready(v);
            }
            Poll::Pending => {}
        }

        match self.f2.as_mut().poll(cx) {
            Poll::Ready(v) => {
                return Poll::Ready(v);
            }
            Poll::Pending => {}
        }

        return Poll::Pending;
    }
}

pub struct Map<F, M> {
    future: F,
    mapper: M,
}

pub fn map<F, M>(future: F, mapper: M) -> Map<F, M> {
    Map { future, mapper }
}

impl<T, Y, F: Future<Output = T> + Unpin, M: Fn(T) -> Y + Unpin> Future for Map<F, M> {
    type Output = Y;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let inst = self.get_mut();

        let p = Pin::new(&mut inst.future);
        match p.poll(cx) {
            Poll::Ready(v) => Poll::Ready((inst.mapper)(v)),
            Poll::Pending => Poll::Pending,
        }
    }
}
