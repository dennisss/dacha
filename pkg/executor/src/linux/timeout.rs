use alloc::boxed::Box;
use common::io::IoError;
use common::tree::comparator::OrdComparator;
use core::task::Poll;
use core::task::Waker;
use core::{future::Future, time::Duration};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use common::errors::*;
use common::hash::FastHasherBuilder;
use common::tree::binary_heap::*;

use crate::channel;
use crate::future::{map, race};
use crate::linux::executor::{ExecutorShared, TaskId};
use crate::linux::io_uring::ExecutorOperation;

use super::thread_local::CurrentExecutorContext;

const MAX_SLEEP_PRECISION: Duration = Duration::from_nanos(500_000); // 0.5ms

/// Timer queue which multiplexes many timeouts on top of a single io uring
/// operation.
///
/// The goal of this is to avoid consuming many io uring operations with
/// timeouts as they would take away slots from more interesting IO operations.
pub(super) struct ExecutorTimeouts {
    shared: Arc<Shared>,
}

/// Unique identifier for a single timeout. These are never repeated and once a
/// timeout is removed from the heap, it is considered to be fulfilled.
type TimeoutId = u64;

struct Shared {
    state: std::sync::Mutex<State>,

    // Used to wake the task that is running to
    sender: channel::Sender<()>,
    receiver: channel::Receiver<()>,
}

struct State {
    last_timeout_id: TimeoutId,

    /// If we have a timer enqueued on the io_uring, then this is when it will
    /// expire.
    ///
    /// When this is None, then no background task is currently running to wait
    /// for timeouts.
    next_expiration: Option<Instant>,

    timeouts_heap: BinaryHeap<(Instant, TimeoutId), OrdComparator, TimeoutHeapIndex>,

    shutting_down: bool,
}

#[derive(Default)]
struct TimeoutHeapIndex {
    entries: HashMap<TimeoutId, TimeoutEntry, FastHasherBuilder>,
}

impl BinaryHeapIndex<(Instant, TimeoutId)> for TimeoutHeapIndex {
    type Query = TimeoutId;

    fn record_offset(&mut self, value: &(Instant, TimeoutId), offset: usize) {
        self.entries
            .entry(value.1)
            .or_insert(TimeoutEntry {
                waker: None,
                heap_index: 0,
            })
            .heap_index = offset;
    }

    fn lookup_offset(&self, query: &Self::Query) -> Option<usize> {
        self.entries.get(query).map(|v| v.heap_index)
    }

    fn clear_offset(&mut self, value: &(Instant, TimeoutId)) {
        self.entries.remove(&value.1);
    }
}

struct TimeoutEntry {
    waker: Option<Waker>,
    heap_index: usize,
}

impl ExecutorTimeouts {
    pub fn new() -> Self {
        let (sender, receiver) = channel::bounded(1);

        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    last_timeout_id: 0,
                    next_expiration: None,
                    timeouts_heap: BinaryHeap::<
                        (Instant, TimeoutId),
                        OrdComparator,
                        TimeoutHeapIndex,
                    >::default(),
                    shutting_down: false,
                }),
                sender,
                receiver,
            }),
        }
    }

    pub fn create_timeout(&self, time: Instant) -> Option<TimeoutId> {
        let mut state = self.shared.state.lock().unwrap();

        // Exit early if the time has already elapsed.
        let now = Instant::now();
        if now >= time {
            return None;
        }

        let id = state.last_timeout_id + 1;
        state.last_timeout_id = id;

        state.timeouts_heap.insert((time, id));

        let new_op = match state.next_expiration {
            Some(t) => t > time,
            None => true,
        };

        let new_task = state.next_expiration.is_none();

        if new_op {
            state.next_expiration = Some(now + Self::get_sleep_duration(now, time));
            if !new_task {
                let _ = self.shared.sender.try_send(());
            }
        }

        if new_task {
            crate::spawn(Self::timeout_waiter_thread(self.shared.clone()));
        }

        Some(id)
    }

    /// Gets the next amount of time we should sleep for before re-checking if
    /// now >= min_timeout.
    ///
    /// Note that we try to only sleep for power of 2 milliseconds to minimize
    /// the worst case number of times that the timeout needs to be re-scheduled
    /// if newer timeouts come in.
    fn get_sleep_duration(now: Instant, min_timeout: Instant) -> Duration {
        let remaining = min_timeout - now;

        let mut dur = MAX_SLEEP_PRECISION;
        while 2 * dur < remaining {
            dur = 2 * dur;
        }

        // Round up the last deadline to minimize the number of sleeps that we need.
        if dur < remaining && dur + MAX_SLEEP_PRECISION >= remaining {
            dur += MAX_SLEEP_PRECISION;
        }

        dur
    }

    async fn timeout_waiter_thread(shared: Arc<Shared>) {
        loop {
            let next_sleep = {
                let mut state = shared.state.lock().unwrap();

                let now = Instant::now();

                let mut next_sleep = None;

                // Pull out all entries that are done now and notify the user.
                while let Some((min_timeout, min_timeout_id)) = state.timeouts_heap.peek_min() {
                    if *min_timeout > now && !state.shutting_down {
                        // Configure the next sleep time.
                        next_sleep = Some(Self::get_sleep_duration(now, *min_timeout));
                        break;
                    }

                    // Timeout has elapsed so clear it and wake the task if needed.

                    let entry = state
                        .timeouts_heap
                        .index()
                        .entries
                        .get(min_timeout_id)
                        .unwrap();
                    if let Some(waker) = &entry.waker {
                        waker.wake_by_ref();
                    }

                    state.timeouts_heap.extract_min();
                }

                match next_sleep {
                    Some(v) => {
                        state.next_expiration = Some(now + v);
                        v
                    }
                    None => {
                        state.next_expiration = None;
                        break;
                    }
                }
            };

            // Perform the actual sleep operation (or wake up early if we get a newer
            // timeout request).

            let sleep_future = Self::sleep_raw(next_sleep);
            let reschedule_future = map(shared.receiver.recv(), |_| Ok(()));

            let res = race!(sleep_future, reschedule_future).await;

            // If this fails, it should only be due to a IoErrorKind::Cancelled error from
            // the executor shutting down.
            if let Err(_) = res {
                shared.state.lock().unwrap().shutting_down = true;
            }
        }
    }

    async fn sleep_raw(duration: Duration) -> Result<()> {
        let op = ExecutorOperation::submit(sys::IoUringOp::Timeout { duration }).await?;
        let res = op.wait().await?;
        res.timeout_result()?;
        Ok(())
    }
}

struct TimeoutFuture {
    timeout_id: Option<TimeoutId>,
}

impl Future for TimeoutFuture {
    type Output = Result<()>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let executor = CurrentExecutorContext::current().unwrap();
        let mut state = executor.timeouts.shared.state.lock().unwrap();

        let timeout_id = match self.timeout_id.clone() {
            Some(v) => v,
            None => return Poll::Ready(Ok(())),
        };

        if state.shutting_down {
            return Poll::Ready(Err(IoError::new(
                common::io::IoErrorKind::Cancelled,
                "Executor timeouts shutting down",
            )
            .into()));
        }

        match state.timeouts_heap.index_mut().entries.get_mut(&timeout_id) {
            Some(entry) => {
                entry.waker = Some(cx.waker().clone());
                Poll::Pending
            }
            None => Poll::Ready(Ok(())),
        }
    }
}

pub fn sleep(duration: Duration) -> impl Future<Output = Result<()>> {
    let executor = CurrentExecutorContext::current().unwrap();

    let mut now = Instant::now();
    let timeout_id = executor.timeouts.create_timeout(now + duration);

    TimeoutFuture { timeout_id }
}

pub fn timeout<F: Future>(duration: Duration, f: F) -> impl Future<Output = Result<F::Output>> {
    race(
        map(Box::pin(f), |v| Ok(v)),
        map(Box::pin(sleep(duration)), |_| {
            Err(err_msg("Future timed out"))
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn one_timeout_at_a_time_test() {
        let start = Instant::now();

        crate::run(async {
            sleep(Duration::from_millis(50)).await.unwrap();
            let t1 = (Instant::now() - start).as_millis() as isize;
            assert!((t1 - 50).abs() < 5);

            sleep(Duration::from_millis(100)).await.unwrap();
            let t1 = (Instant::now() - start).as_millis() as isize;
            assert!((t1 - 150).abs() < 5);

            sleep(Duration::from_millis(10)).await.unwrap();
            let t1 = (Instant::now() - start).as_millis() as isize;
            assert!((t1 - 160).abs() < 5);
        })
        .unwrap();
    }

    #[test]
    fn concurrent_timeouts() {
        // Here we are trying to test that if we get a timeout request that is much
        // sooner than any existing timeouts, we currently adjust the amount of time
        // that we wait.

        let start = Instant::now();

        crate::run(async {
            let f1 = sleep(Duration::from_millis(200));

            // Give the background thread some time to start waiting for the first timeout.
            std::thread::sleep(Duration::from_millis(10));

            let f2 = sleep(Duration::from_millis(10));

            f2.await.unwrap();

            let t1 = (Instant::now() - start).as_millis() as isize;
            assert!((t1 - 20).abs() < 5);

            f1.await.unwrap();

            let t1 = (Instant::now() - start).as_millis() as isize;
            assert!((t1 - 200).abs() < 5);
        })
        .unwrap();
    }
}
