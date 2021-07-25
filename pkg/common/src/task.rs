use async_std::task::JoinHandle;


/// A task which stops running once it is dropped.
pub struct ChildTask<T: 'static + Send = ()> {
    handle: Option<JoinHandle<T>>
}

impl<T: 'static + Send> ChildTask<T> {
    pub fn spawn<Fut: 'static + std::future::Future<Output = T> + Send>(future: Fut) -> Self {
        Self {
            handle: Some(async_std::task::spawn(future))
        }
    }

    pub async fn join(mut self) -> T {
        let handle = self.handle.take().unwrap();
        handle.await
    }
}

impl<T: 'static + Send> Drop for ChildTask<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            async_std::task::spawn(handle.cancel());
        }
    }
}