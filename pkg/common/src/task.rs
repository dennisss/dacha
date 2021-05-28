use async_std::task::JoinHandle;


/// A task which stops running once it is dropped.
pub struct ChildTask {
    handle: Option<JoinHandle<()>>
}

impl ChildTask {
    pub fn spawn<Fut: 'static + std::future::Future<Output = ()> + Send>(future: Fut) -> Self {
        Self {
            handle: Some(async_std::task::spawn(future))
        }
    }
}

impl Drop for ChildTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            async_std::task::spawn(handle.cancel());
        }
    }
}