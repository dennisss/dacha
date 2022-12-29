use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::sync::Mutex;

/// Error
// pub struct ChannelClosed;

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Mutex::new(Inner {
        value: None,
        sender_alive: true,
        receiver_alive: true,
        receiver_waker: None,
    }));
    let inner2 = inner.clone();

    (
        Sender { inner: Some(inner) },
        Receiver {
            inner: Some(inner2),
        },
    )
}

struct Inner<T> {
    value: Option<T>,
    sender_alive: bool,
    receiver_alive: bool,
    receiver_waker: Option<Waker>,
}

pub struct Sender<T> {
    inner: Option<Arc<Mutex<Inner<T>>>>,
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let mut guard = inner.lock().unwrap();
            guard.sender_alive = false;

            if let Some(waker) = guard.receiver_waker.take() {
                waker.wake();
            }
        }
    }
}

impl<T> Sender<T> {
    /// NOTE: This does not guarantee that the receiver will ever pull the
    /// value.
    pub fn send(mut self, value: T) -> Result<(), T> {
        let inner = self.inner.take().unwrap();
        let mut guard = inner.lock().unwrap();
        if !guard.receiver_alive {
            return Err(value);
        }

        guard.value = Some(value);

        if let Some(waker) = guard.receiver_waker.take() {
            waker.wake();
        }

        Ok(())
    }
}

pub struct Receiver<T> {
    inner: Option<Arc<Mutex<Inner<T>>>>,
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let mut guard = inner.lock().unwrap();
            guard.receiver_alive = false;
        }
    }
}

impl<T> Receiver<T> {
    pub fn recv(mut self) -> impl Future<Output = Result<T, ()>> + Unpin {
        RecvFuture { receiver: self }
    }
}

struct RecvFuture<T> {
    receiver: Receiver<T>,
}

impl<T> Future for RecvFuture<T> {
    type Output = Result<T, ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut guard = this.receiver.inner.as_ref().unwrap().lock().unwrap();

        if let Some(value) = guard.value.take() {
            drop(guard);
            this.receiver.inner.take();
            return Poll::Ready(Ok(value));
        }

        if !guard.sender_alive {
            drop(guard);
            this.receiver.inner.take();
            return Poll::Ready(Err(()));
        }

        guard.receiver_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oneshot_send_before_receive_test() {
        crate::run(async {
            let (sender, receiver) = channel::<u32>();
            assert_eq!(sender.send(12), Ok(()));
            assert_eq!(receiver.recv().await, Ok(12));
        })
        .unwrap();
    }

    #[test]
    fn oneshot_receive_before_send_test() {
        crate::run(async {
            let (sender, receiver) = channel::<u32>();

            let joiner = crate::spawn(async move {
                crate::sleep(std::time::Duration::from_millis(10)).await;
                assert_eq!(sender.send(22), Ok(()));
            });

            assert_eq!(receiver.recv().await, Ok(22));

            joiner.join().await;
        })
        .unwrap();
    }

    #[test]
    fn oneshot_sender_dropped() {
        crate::run(async {
            let (sender, receiver) = channel::<u32>();
            drop(sender);
            assert_eq!(receiver.recv().await, Err(()));
        })
        .unwrap();
    }

    #[test]
    fn oneshot_reciever_dropped() {
        crate::run(async {
            let (sender, receiver) = channel::<u32>();
            drop(receiver);
            assert_eq!(sender.send(100), Err(100));
        })
        .unwrap();
    }
}
