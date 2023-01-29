# Async Executor

This library enables executing async futures. It is responsible for:

- Maintaining thread pools
- Managing I/O requests with the OS.
- Multiplex execution of `Task`s which are wrappers around independent Futures which are pooled concurrently.
- Provides cooperative multitasking style primitives (no support for task pre-emption).

## Linux Executor

When running on Linux, this internally uses io_uring and is designed as follows:

- Tasks are scheduled to run using a thread pool of N tasks.
- A task blocks its assigned thread while Future::poll() is running.
- When a task needs to do async I/O it will submit an entry to the io_uring submission queue.
- Then the running thread will set aside the task and execute any other pending task.
- One dedicated thread continuously waits for completion of entries on the io_uring completion queue.
- When a completion is dedicated, the requesting task is re-enqueued to run.

### Usage

The recommended way to use the executor is via the `#[executor_main]` macro which wraps a `main()` function:

```rust
#[executor_main]
async fn main() {
    // Example of spawning concurrently executing futures.
    let join_handle = executor::spawn(async move {
        executor::sleep(Duration::from_secs(1)).await;
        123
    });

    let result = join_handle.join().await;

    println!("{}", result); // Will print '123'
}
```

The above `main()` function will exit shortly after the root future executed in it has finished.
Additionally it will shortly wait for any cleanup tasks to finish running (see the
[shutdown behavior](#shutdown)). Any other tasks still running will stop running with the
termaination of the process.

Note that during process termination, we never 'cancel' tasks. This could risk violating the
expectations mentioned in the next section regarding critical sections.

### Critical Sections

There are times at which we need to execute some async operations with a guarantee that the operation will run til completion without cancellation (or the future getting dropped before returning a Poll::Ready). For example, if a Mutex lock is held across an async wait, it may not be guaranteed that all mutations to the locked data happen. To support this use-case, we guarantee that a task spawned with `executor::spawn()` where the returned `JoinHandle` is dropped will never be canceled. So if you do require creating a critical section, you'd need to explicitly spawn a new task and ignore its handle.

### Multiple Tasks

A common pattern with servers is to execute multiple different tasks which may fail all in parallel. To fascilitate this, we provide the `TaskResultBundle` type.

TODO: Provide an example

TODO: Document when a dependency should use provide a `run()` vs spawning its own background tasks internally (depends on error propagation).

### Shutdown Behaviors {#shutdown}

Some event loops (like those waiting for incoming TCP connections) don't have a clearly defined termination
condition. To still facilitate gracefully stopping such programs, each loop should acquire a
`CancellationToken` instance which allows it to check whether or not the program should continue
running.

The default token implementation can be retried by calling `executor::signals::new_shutdown_token()`
and should be sufficient for most usecases. This token will shutdown the program when
SIGINT/SIGTERM (e.g. Ctrl-C) signals are recieved.

TODO: Implement scoped cancellation tokens (e.g. so that tasks can start cancellable sub-trees for things like unit tests).

## Cortex-M Executor

For ARM Cortex-M microcontrollers, the executor is designed to support heapless (no alloc, no_std) operation using async operations driven by CPU interrupts:

- We assume the microcontroller only has a single CPU core.
- Every task that can run is stored in static mutable variables or on the stack.
- When a task runs, interrupts are disabled so it will never be pre-empted until it yields control of the thread.
- When a task needs to perform an async operation, it will:
    1. Setup MCU registers to trigger a future interrupt invocation.
    2. Adds the task's id to a waker list for the specific interrupt type.
- Later, the interrupt handler will iterative over the waker list associated with the received interrupt type and will poll each task once.

The 'waker lists' mentioned above are designed as follows to ensure no-alloc operation:

- We define at compile time a fixed number of waker lists in static variables equal to the number of CPU interrupts.
- Each waker list is a linked list with initially an empty head.
- When a task wants to add an entry to a list, it creates a stack-pinned link list entry and chains it to the end of the list.
- On drop of the list entry, it will remove itself from the list to ensure memory safety.
