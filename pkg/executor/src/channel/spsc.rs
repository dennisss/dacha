use core::fmt::{Debug, Display};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::collections::VecDeque;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

use crate::channel::error::*;

/// Channel which supports a single producer and single consumer instance.
/// Messages are guaranteed to be received in the order they are sent.
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let mut values = VecDeque::new();
    values.reserve_exact(capacity);

    let inner = Arc::new(Mutex::new(Inner {
        values,
        capacity,
        corked: false,
        sender_waker: None,
        sender_alive: true,
        receiver_waker: None,
        receiver_alive: true,
    }));

    let sender = Sender {
        inner: inner.clone(),
    };
    let receiver = Receiver { inner };
    (sender, receiver)
}

struct Inner<T> {
    values: VecDeque<T>,

    capacity: usize,

    /// If true, receivers will not attempt to pull any values.
    corked: bool,

    sender_waker: Option<Waker>,

    sender_alive: bool,

    receiver_waker: Option<Waker>,

    receiver_alive: bool,
}

pub struct Sender<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.sender_alive = false;
        inner.sender_waker.take();
        inner.corked = false;
        if let Some(waker) = inner.receiver_waker.take() {
            waker.wake();
        }
    }
}

impl<T> Sender<T> {
    pub fn send<'a>(
        &'a mut self,
        value: T,
    ) -> impl Future<Output = Result<(), SendRejected<T>>> + 'a {
        SendFuture {
            value: Some(value),
            sender: self,
        }
    }

    pub fn try_send(&mut self, value: T) -> Result<(), SendRejected<T>> {
        let mut inner = self.inner.lock().unwrap();
        Self::try_send_impl(&mut inner, value)
    }

    fn try_send_impl(inner: &mut Inner<T>, value: T) -> Result<(), SendRejected<T>> {
        inner.sender_waker = None;

        if !inner.receiver_alive {
            return Err(SendRejected {
                value,
                error: SendError::ReceiverDropped,
            });
        }

        if inner.values.len() < inner.capacity {
            inner.values.push_back(value);

            if !inner.corked {
                if let Some(waker) = inner.receiver_waker.take() {
                    waker.wake();
                }
            }

            return Ok(());
        }

        Err(SendRejected {
            value,
            error: SendError::OutOfSpace,
        })
    }

    /// Returns the number of unacknowledged values in the channel.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().values.len()
    }

    pub fn capacity(&self) -> usize {
        self.inner.lock().unwrap().capacity
    }

    ///
    /// A corked channel will become uncorked when the sender is dropped.
    ///
    /// NOTE: While corked, the sender should pay close attention to the number
    /// of elements in the channel to prevent deadlock.
    ///
    /// TODO: We must uncork on drop to ensure there is no deadlock.
    pub fn cork(&mut self) {
        self.inner.lock().unwrap().corked = true;
    }

    pub fn uncork(&mut self) {
        self.inner.lock().unwrap().corked = false;
        // TODO: Maybe notify people here.
    }
}

struct SendFuture<'a, T> {
    sender: &'a mut Sender<T>,
    value: Option<T>,
}

impl<'a, T> Future for SendFuture<'a, T> {
    type Output = Result<(), SendRejected<T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut inner = this.sender.inner.lock().unwrap();
        match Sender::try_send_impl(&mut inner, this.value.take().unwrap()) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => {
                if e.error == SendError::OutOfSpace && !inner.corked {
                    this.value = Some(e.value);
                    inner.sender_waker = Some(cx.waker().clone());
                    return Poll::Pending;
                }

                Poll::Ready(Err(e))
            }
        }
    }
}

pub struct Receiver<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.receiver_alive = false;
        inner.receiver_waker.take();
        if let Some(waker) = inner.sender_waker.take() {
            waker.wake();
        }
    }
}

impl<T> Receiver<T> {
    pub fn capacity(&self) -> usize {
        self.inner.lock().unwrap().capacity
    }

    /// If this fails, then the sender was dropped.
    pub fn recv<'a>(&'a mut self) -> impl Future<Output = Result<T, RecvError>> + 'a {
        RecvFuture { receiver: self }
    }

    pub fn try_recv(&mut self) -> Option<Result<T, RecvError>> {
        let mut inner = self.inner.lock().unwrap();
        Self::try_recv_impl(&mut inner)
    }

    fn try_recv_impl(inner: &mut Inner<T>) -> Option<Result<T, RecvError>> {
        inner.receiver_waker = None;

        // NOTE: This is set to false when the receiver is dropped, so this will always
        // eventually be false.
        if inner.corked {
            return None;
        }

        if let Some(value) = inner.values.pop_front() {
            if let Some(waker) = inner.sender_waker.take() {
                waker.wake();
            }

            return Some(Ok(value));
        }

        if !inner.sender_alive {
            return Some(Err(RecvError::SenderDropped));
        }

        None
    }

    /// Blocks until try_recv() would return a non-None result without actually
    /// mutating the state of the channel.
    pub fn wait<'a>(&'a mut self) -> impl Future<Output = ()> + 'a {
        WaitFuture { receiver: self }
    }

    fn try_wait_impl(inner: &mut Inner<T>) -> bool {
        inner.receiver_waker = None;

        // NOTE: This is set to false when the receiver is dropped, so this will always
        // eventually be false.
        if inner.corked {
            return false;
        }

        if !inner.values.is_empty() {
            return true;
        }

        if !inner.sender_alive {
            return true;
        }

        false
    }
}

struct RecvFuture<'a, T> {
    receiver: &'a mut Receiver<T>,
}

impl<'a, T> Future for RecvFuture<'a, T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut inner = this.receiver.inner.lock().unwrap();

        if let Some(value) = Receiver::try_recv_impl(&mut inner) {
            return Poll::Ready(value);
        }

        inner.receiver_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

struct WaitFuture<'a, T> {
    receiver: &'a mut Receiver<T>,
}

impl<'a, T> Future for WaitFuture<'a, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut inner = this.receiver.inner.lock().unwrap();

        if Receiver::try_wait_impl(&mut inner) {
            return Poll::Ready(());
        }

        inner.receiver_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
