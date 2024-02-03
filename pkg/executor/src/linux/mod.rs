// pub mod channel;

// mod polling;

mod epoll;
mod error;
mod executor;
mod file;
mod io_uring;
mod join_handle;
mod options;
mod task;
mod thread_local;
mod timeout;
mod utils;
mod waker;
mod yielding;

pub use self::executor::TaskId;
pub use epoll::ExecutorPollingContext;
pub use error::*;
pub use file::{FileHandle, SyncRange};
pub use io_uring::ExecutorOperation;
pub use join_handle::*;
pub use options::*;
pub use task::Task;
pub use timeout::*;
pub use utils::*;
pub use yielding::yield_now;

/*
TODO: Write a design doc for this.

Some things to define:
- Executor
    - Contains a thread pool (one per CPU core)
        - Eventually do an explicit locking of each to a different core.
    - Given the initial future and sends it to one thread.
    - One thread with be doing all polling and feeding to a work queue

Main Thread:
- Continuously running epoll on the set of files in the Executor
- When a file is ready, find the task associated with it and push task's id into the queue

Tasks are boxed futures with an id
- These are stored in a slab (though we also want to have unique task ids)

- Waker passed a pointer to the location in the slab
    -

Pooling a future:
- First try to read/write from the file descriptor.
- If we need to block then:
    - Context contains a Waker / RawWaker which has an Arc<TaskEntry>
    - Access the ExecutorShared state.
    - Add the file to the set to be queued (and mark that this task is waiting on them)
    - Hit an eventfd to notify the main thread to wait on these new ones.


First use-case:
- File read (POLLIN and POLLHUP?)

Implementing an async channel:
- Has nothing to do with io_uring.
- Basically just need an async cond var.
    - Ideally use the same futex used to wait for the pending_queue changes.

*/
