use std::future::Future;
use std::task::{Poll, Context};
use std::pin::Pin;

use futures::channel::oneshot;

/// Wrapper around an object which allows temporarily giving away the object to another
/// function and then later getting it back once it is no longer in use.
pub struct Borrowed<T> {
    inner: Option<(T, oneshot::Sender<T>)>
}

impl<T> Borrowed<T> {
    pub fn wrap(value: T) -> (Self, BorrowedReturner<T>) {
        let (sender, receiver) = oneshot::channel();
        (Self { inner: Some((value, sender)) }, BorrowedReturner { receiver })
    }
}

impl<T> Drop for Borrowed<T> {
    fn drop(&mut self) {
        let (body, sender) = self.inner.take().unwrap();
        // NOTE: It's ok if the receiver was dropped.
        let _ = sender.send(body);
    }
}

impl<T> std::ops::Deref for Borrowed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.as_ref().unwrap().0
    }
}

impl<T> std::ops::DerefMut for Borrowed<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.as_mut().unwrap().0
    }
}

pub struct BorrowedReturner<T> {
    receiver: oneshot::Receiver<T>
}

impl<T> Future for BorrowedReturner<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // NOTE: The unwrap() should never panic as the sender is always used before being dropped.
        Pin::new(&mut self.receiver).poll(cx)
            .map(|r| r.unwrap())
    }
}