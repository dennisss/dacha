use crate::errors::*;
//use crate::futures::stream::{Stream, StreamExt};
use core::task::Context;
use futures::future::Select;
use futures::task::Poll;
use futures::Stream;
use futures::StreamExt;
use std::future::Future;
use std::pin::Pin;

const BUF_SIZE: usize = 4096;

#[async_trait]
pub trait Streamable: Send {
    type Item: 'static + Send;
    async fn next(&mut self) -> Option<Self::Item>;
}

//#[async_trait]
//impl<S: crate::futures::stream::Stream + Send + Unpin> Streamable for S
//where
//    <S as crate::futures::stream::Stream>::Item: Send + 'static,
//{
//    type Item = <S as crate::futures::stream::Stream>::Item;
//    async fn next(&mut self) -> Option<Self::Item> {
//        <S as crate::futures::stream::StreamExt>::next(self).await
//    }
//}

//#[async_trait]
//impl<S: Streamable + Unpin + ?Sized> Streamable for Box<S> {
//    type Item = S::Item;
//    async fn next(&mut self) -> Option<Self::Item> {
//        self.as_mut().next().await
//    }
//}

//#[async_trait]
//impl<S: Streamable> Streamable for Pin<S> {
//    type Item = S::Item;
//    async fn next(&mut self) -> Option<Self::Item> {
//        self.as_mut().next().await
//    }
//}

pub trait StreamExt2: Stream + Sized + Send {
    fn bind_then<
        T: 'static + Send,
        C: 'static + Send + Sync + Clone,
        Fut: 'static + Future<Output = T> + Send,
        F: Send + FnMut(C, Self::Item) -> Fut,
    >(
        self,
        ctx: C,
        f: F,
    ) -> BoundThenStreamable<Self, F, C> {
        BoundThenStreamable {
            inner: self,
            f,
            ctx,
        }
    }

    //    fn select<S: Streamable<Item = Self::Item> + 'static>(
    //        self,
    //        other: S,
    //    ) -> SelectStreamable<Self, S>
    //    where
    //        Self: 'static,
    //    {
    //        SelectStreamable {
    //            stream_a: Some(self.into_future()),
    //            stream_b: Some(other.into_future()),
    //        }
    //    }
}

impl<S: Stream + Sized + Send> StreamExt2 for S {}

#[async_trait]
pub trait StreamableExt: Streamable + Sized {
    /// Converts the element type produced by this stream using a synchronous
    /// converter.
    fn map<T: 'static + Send, F: Send + FnMut(Self::Item) -> T>(
        self,
        f: F,
    ) -> MapStreamable<Self, F> {
        MapStreamable { inner: self, f }
    }

    fn then<
        T: 'static + Send,
        Fut: 'static + Future<Output = T> + Send,
        F: Send + FnMut(Self::Item) -> Fut,
    >(
        self,
        f: F,
    ) -> ThenStreamable<Self, F> {
        ThenStreamable { inner: self, f }
    }

    async fn into_future(mut self) -> (Option<Self::Item>, Self) {
        let v = self.next().await;
        (v, self)
    }

    fn into_stream(self) -> IntoStream<Self>
    where
        Self: 'static,
    {
        IntoStream {
            inner: Some(self.into_future()),
        }
    }
}

impl<S: Streamable> StreamableExt for S {}

pub struct IntoStream<S: Streamable> {
    inner: Option<Pin<Box<dyn Future<Output = (Option<S::Item>, S)> + Send + 'static>>>,
}

impl<S: 'static + Streamable> futures::stream::Stream for IntoStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(mut fut) = self.inner.take() {
            if let Poll::Ready((v, s)) = fut.as_mut().poll(cx) {
                self.inner = Some(s.into_future());
                Poll::Ready(v)
            } else {
                self.inner = Some(fut);
                Poll::Pending
            }
        } else {
            Poll::Ready(None)
        }
    }
}

pub struct MapStreamable<S, F> {
    inner: S,
    f: F,
}

#[async_trait]
impl<S: Streamable, T: 'static + Send, F: Send + FnMut(S::Item) -> T> Streamable
    for MapStreamable<S, F>
{
    type Item = T;
    async fn next(&mut self) -> Option<Self::Item> {
        let v = self.inner.next().await;
        v.map(&mut self.f)
    }
}

pub struct ThenStreamable<S, F> {
    inner: S,
    f: F,
}

#[async_trait]
impl<
        S: Streamable,
        T: 'static + Send,
        Fut: 'static + Future<Output = T> + Send,
        F: Send + FnMut(S::Item) -> Fut,
    > Streamable for ThenStreamable<S, F>
{
    type Item = T;
    async fn next(&mut self) -> Option<Self::Item> {
        let v = self.inner.next().await;
        if let Some(value) = v {
            Some((self.f)(value).await)
        } else {
            None
        }
    }
}

pub struct BoundThenStreamable<S, F, C> {
    inner: S,
    f: F,
    ctx: C,
}

#[async_trait]
impl<
        S: Stream + Send + Unpin,
        T: 'static + Send,
        C: 'static + Send + Sync + Clone,
        Fut: 'static + Future<Output = T> + Send,
        F: Send + FnMut(C, S::Item) -> Fut,
    > Streamable for BoundThenStreamable<S, F, C>
where
    S::Item: Send + 'static,
{
    type Item = T;
    async fn next(&mut self) -> Option<Self::Item> {
        let v = self.inner.next().await;
        if let Some(value) = v {
            Some((self.f)(self.ctx.clone(), value).await)
        } else {
            None
        }
    }
}

pub struct SelectStreamable<Sa: Streamable, Sb: Streamable> {
    stream_a: Option<Pin<Box<dyn Future<Output = (Option<Sa::Item>, Sa)> + Send + 'static>>>,
    stream_b: Option<Pin<Box<dyn Future<Output = (Option<Sb::Item>, Sb)> + Send + 'static>>>,
}

// Captures and returns a Future<>

#[async_trait]
impl<Sa: Streamable + 'static, Sb: Streamable<Item = Sa::Item> + 'static> Streamable
    for SelectStreamable<Sa, Sb>
{
    type Item = Sa::Item;
    async fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.stream_a.is_none() && self.stream_b.is_none() {
                // NOTE: Currently this should never happen.
                return None;
            }
            if self.stream_a.is_none() {
                let (v, b) = self.stream_b.take().unwrap().await;
                self.stream_b = Some(b.into_future());
                return v;
            }
            if self.stream_b.is_none() {
                let (v, a) = self.stream_a.take().unwrap().await;
                self.stream_a = Some(a.into_future());
                return v;
            }

            let either = futures::future::select(
                self.stream_a.take().unwrap(),
                self.stream_b.take().unwrap(),
            )
            .await;
            match either {
                futures::future::Either::Left(((item, sa), b)) => {
                    self.stream_b = Some(b);
                    if item.is_some() {
                        self.stream_a = Some(sa.into_future());
                        return item;
                    }
                }
                futures::future::Either::Right(((item, sb), a)) => {
                    self.stream_a = Some(a);
                    if item.is_some() {
                        self.stream_b = Some(sb.into_future());
                        return item;
                    }
                }
            }
        }
    }
}

//pub struct VecStream<T: 'static + Send> {
//    items: Vec<T>,
//    pos: usize,
//}
//
//impl<T: 'static + Send + Clone> Stream<Option<T>> for VecStream<T> {
//    async fn next(&mut self) -> Option<T> {
//        if self.pos < self.items.len() {
//            Some(self.items[i].clone())
//        } else {
//            None
//        }
//    }
//}
//
//pub fn stream_vec<T: 'static + Send + Clone>(
//    items: Vec<T>,
//) -> Pin<Box<dyn Stream<Option<T>> + Send + 'static>> {
//    Box::pin(VecStream { items, pos: 0 })
//}

//pub struct ThenStream {
//    handler:
//}

//pub fn stream_then<Y: 'static + Send, C: 'static + Send, T: 'static + Send,
// F>(    stream: Pin<Box<dyn Stream<Option<T>> + Send + 'static>>,
//    context: C,
//    func: F,
//) -> Pin<Box<dyn Stream<Option<T>> + Send + 'static>>
//where
//    for<'a> F: crate::async_fn::AsyncFn2<&'a C, T, Output = Y>,
//{
//}

#[async_trait]
pub trait Sinkable<T: 'static + Send>: Send {
    type Error = Error;
    async fn send(&mut self, value: T) -> std::result::Result<(), Self::Error>;
}

#[async_trait]
impl<T: 'static + Send, S: crate::futures::sink::Sink<T> + Send + Unpin> Sinkable<T> for S {
    type Error = S::Error;
    async fn send(&mut self, value: T) -> std::result::Result<(), Self::Error> {
        <S as crate::futures::sink::SinkExt<T>>::send(self, value).await
    }
}

/// An asynchronously readable object. Works similarly to std::io::Read except
/// allows multiple readers to operate simultaneously on the object. The
/// internal implementation is responsible for ensuring that any necessary
/// locking is performed.
#[async_trait]
pub trait Readable: Send + Sync + Unpin + 'static {
    async fn read(&self, buf: &mut [u8]) -> Result<usize>;

    // TODO: Deduplicate for http::Body
    async fn read_to_end(&self, buf: &mut Vec<u8>) -> Result<()> {
        let mut i = buf.len();
        loop {
            buf.resize(i + BUF_SIZE, 0);

            let res = self.read(&mut buf[i..]).await;
            match res {
                Ok(n) => {
                    i += n;
                    if n == 0 {
                        buf.resize(i, 0);
                        return Ok(());
                    }
                }
                Err(e) => {
                    buf.resize(i, 0);
                    return Err(e);
                }
            }
        }
    }

    async fn read_exact(&self, mut buf: &mut [u8]) -> Result<()> {
        while buf.len() > 0 {
            let n = self.read(buf).await?;
            if n == 0 {
                return Err(err_msg("Underlying stream closed"));
            }

            buf = &mut buf[n..];
        }

        Ok(())
    }
}

#[async_trait]
pub trait Writeable: Send + Sync + Unpin + 'static {
    async fn write(&self, buf: &[u8]) -> Result<usize>;

    async fn flush(&self) -> Result<()>;

    async fn write_all(&self, mut buf: &[u8]) -> Result<()> {
        while buf.len() > 0 {
            let n = self.write(buf).await?;
            if n == 0 {
                return Err(err_msg("Underlying stream closed"));
            }

            buf = &buf[n..];
        }

        Ok(())
    }
}

#[async_trait]
impl Readable for async_std::net::TcpStream {
    async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut r = self;
        let n = async_std::io::prelude::ReadExt::read(&mut r, buf).await?;
        Ok(n)
    }
}

#[async_trait]
impl Writeable for async_std::net::TcpStream {
    async fn write(&self, buf: &[u8]) -> Result<usize> {
        let mut r = self;
        let n = async_std::io::prelude::WriteExt::write(&mut r, buf).await?;
        Ok(n)
    }

    async fn flush(&self) -> Result<()> {
        let mut r = self;
        async_std::io::prelude::WriteExt::flush(&mut r).await?;
        Ok(())
    }
}

pub trait ReadWriteable: Readable + Writeable {
    fn as_read(&self) -> &dyn Readable;
    fn as_write(&self) -> &dyn Writeable;
}

impl<T: Readable + Writeable> ReadWriteable for T {
    fn as_read(&self) -> &dyn Readable {
        self
    }
    fn as_write(&self) -> &dyn Writeable {
        self
    }
}
