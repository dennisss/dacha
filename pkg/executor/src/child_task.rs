use crate::{spawn, JoinHandle, Task};

/// A task which stops running once it is dropped.
pub struct ChildTask<T: 'static + Send = ()> {
    handle: JoinHandle<T>,
}

impl<T: 'static + Send> ChildTask<T> {
    pub fn spawn<Fut: 'static + std::future::Future<Output = T> + Send>(future: Fut) -> Self {
        let mut handle = spawn(future);
        handle.attach();

        Self { handle }
    }

    pub fn task(&self) -> &Task {
        self.handle.task()
    }

    pub async fn join(mut self) -> T {
        self.handle.join().await
    }

    pub async fn cancel(mut self) -> Option<T> {
        self.handle.cancel().await
    }
}
