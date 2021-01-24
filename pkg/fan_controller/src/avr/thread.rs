use crate::avr::*;
use core::cell::UnsafeCell;
use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::Poll;

const MAX_NUM_THREADS: usize = 2;

// TODO: Implement support for sleeping
// (e.g. when the computer is off or not receiving any USB traffic).

/// Type used to represent the id of a thread.
///
/// This is also the index of the thread into the internal RUNNING_THREADS
/// array.
///
/// NOTE: This implies that we can have up to 256 threads.
pub type ThreadId = u8;

/// Reference to the thread's
#[derive(Clone, Copy)]
struct ThreadReference {
    /// Type erased Future<Output=()> which contains the state of the thread.
    future: *mut (),

    /// Function which can be passed the above 'future' to poll/wake the thread.
    poll_fn: fn(*mut ()) -> Poll<()>,
}

// TODO: Currently this is incorrect. We need a way of having per-channel change
// events.
// static mut CHANNEL_CHANGED: bool = false;

struct ThreadVec {
    values: [Option<ThreadReference>; MAX_NUM_THREADS],
    length: usize,
}

impl ThreadVec {
    const fn new() -> Self {
        Self {
            values: [None; MAX_NUM_THREADS],
            length: 0,
        }
    }

    fn get(&self, index: ThreadId) -> &ThreadReference {
        self.values[index as usize].as_ref().unwrap()
    }

    #[inline(always)]
    fn push(&mut self, value: ThreadReference) -> ThreadId {
        for i in 0..self.values.len() {
            if self.values[i].is_none() {
                self.values[i] = Some(value);
                self.length += 1;
                return i as ThreadId;
            }
        }

        panic!();
    }

    fn remove(&mut self, index: ThreadId) {
        // assert!(self.values[index as usize].take().is_some());
        self.values[index as usize] = None;
        self.length -= 1;
    }

    fn is_empty(&self) -> bool {
        self.length == 0
    }

    fn iter(&self) -> impl Iterator<Item = (ThreadId, &ThreadReference)> {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.as_ref().clone().map(|r| (index as ThreadId, r)))
    }
}

static mut RUNNING_THREADS: ThreadVec = ThreadVec::new();

static mut CURRENT_THREAD_ID: Option<ThreadId> = None;

static mut THREADS_INITIALIZED: bool = false;

pub struct Thread<Fut: 'static + Sized + Future<Output = ()>> {
    fut: Option<Fut>,
    id: ThreadId,
}

impl<Fut: 'static + Sized + Future<Output = ()>> Thread<Fut> {
    pub const fn new(/* f: fn() -> Fut */) -> Self {
        Self {
            // f: f,
            fut: None,
            id: 0,
        }
    }

    #[inline(always)]
    pub fn start(&'static mut self, f: fn() -> Fut) {
        // Return if already started.
        if self.fut.is_some() {
            return;
        }

        self.fut = Some(f());

        self.id = unsafe {
            RUNNING_THREADS.push(ThreadReference {
                poll_fn: Self::poll_future,
                future: core::mem::transmute(self.fut.as_mut().unwrap()),
            })
        };

        unsafe {
            if THREADS_INITIALIZED {
                poll_thread(self.id);
            }
        }
    }

    fn poll_future(ptr: *mut ()) -> Poll<()> {
        let fut: &mut Fut = unsafe { core::mem::transmute(ptr) };

        // static waker: Waker = unsafe { Waker::from_raw(RAW_WAKER) };
        // static mut cx: Context = Context::from_waker(&waker);

        // TODO: Does this waker have to live for the netier life?
        let waker = unsafe { Waker::from_raw(RAW_WAKER) };
        let mut cx = Context::from_waker(&waker);
        let p = unsafe { Pin::new_unchecked(fut) };
        p.poll(unsafe { &mut cx })
    }

    pub fn stop(&'static mut self) {
        if !self.fut.is_some() {
            return;
        }

        // A thread stopping itself is undefined behavior and should panic.
        assert_ne!(unsafe { CURRENT_THREAD_ID }, Some(self.id));

        // Drop all variables. This should also drop any WakerFutures used by the thread
        // (thus ensuring that this thread id is safe to re-use later).
        self.fut = None;

        unsafe { RUNNING_THREADS.remove(self.id) };
    }
}

pub fn current_thread_id() -> ThreadId {
    unsafe { CURRENT_THREAD_ID.unwrap() }
}

#[macro_export]
macro_rules! define_thread {
    ($(#[$meta:meta])* $name: ident, $handler: expr) => {
        $(#[$meta])*
        struct $name {}

        impl $name {
            #[inline(always)]
            fn ptr() /* -> &'static mut $crate::avr::thread::Thread<impl ::core::future::Future<Output=()>> */ {
                type RetType = impl ::core::future::Future<Output = ()>;
                #[inline(never)]
                fn handler_wrap() -> RetType {
                    ($handler)()
                }

                static mut THREAD: $crate::avr::thread::Thread<RetType> = {
                    $crate::avr::thread::Thread::new()
                };

                unsafe { THREAD.start(handler_wrap) };

                // unsafe { &mut THREAD }
            }

            #[inline(always)]
            pub fn start() {
                Self::ptr();
            }

            // TODO: If a thread is stopped while one thread is running, we may want to intentionally run an extra cycle to ensure that we re-process them.

            // pub fn stop() {
            //     let thread = Self::ptr();
            //     thread.stop();
            // }
        }
    };
}

pub static mut IDLE_COUNTER: u32 = 0;

#[no_mangle]
#[inline(never)]
pub fn block_on_threads() -> ! {
    if unsafe { RUNNING_THREADS.is_empty() } {
        panic!();
    }

    crate::avr::waker::init();

    unsafe {
        THREADS_INITIALIZED = true;

        // crate::avr::serial::uart_send_sync(b"AA\n");

        // Poll all threads for the first time so that wakers can be initialized.
        // TODO: Verify that this works even if the threads start more threads.
        for (id, _thread) in RUNNING_THREADS.iter() {
            crate::avr::serial::uart_send_sync(b"POLL ");
            crate::avr::serial::uart_send_number_sync(id);
            crate::avr::serial::uart_send_sync(b"\n");

            unsafe { poll_thread(id) };
        }

        // enable_interrupts();
        crate::avr::serial::uart_send_sync(b"DONE\n");

        loop {
            llvm_asm!("nop");
        }

        // crate::avr::subroutines::avr_idle_loop(&mut IDLE_COUNTER);
    }

    // NOTE: This should never be reached as the idle_loop should never return.
    panic!();
}

use core::task::{Context, RawWaker, RawWakerVTable, Waker};

unsafe fn raw_waker_clone(data: *const ()) -> RawWaker {
    RAW_WAKER
}

unsafe fn raw_waker_wake(data: *const ()) {
    // panic!();
}

unsafe fn raw_waker_wake_by_ref(data: *const ()) {
    // panic!();
}

unsafe fn raw_waker_drop(data: *const ()) {}

const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    raw_waker_clone,
    raw_waker_wake,
    raw_waker_wake_by_ref,
    raw_waker_drop,
);

const RAW_WAKER: RawWaker = RawWaker::new(0 as *const (), &RAW_WAKER_VTABLE);

/// This function is unsafe as it must only be run when interrupts are
/// disabled.
pub unsafe fn poll_thread(thread_id: ThreadId) {
    // TODO: Assert interrupts are disabled.

    let thread_ref = RUNNING_THREADS.get(thread_id);

    // Should not be polling within a thread
    assert!(CURRENT_THREAD_ID.is_none());
    CURRENT_THREAD_ID = Some(thread_id);

    let result = (thread_ref.poll_fn)(thread_ref.future);

    CURRENT_THREAD_ID = None;

    if result.is_ready() {
        // TODO: Also set the future to None in the thread instance?
        // TODO: Must also ensure that all events are cleaned up
        RUNNING_THREADS.remove(thread_id);
    }

    if RUNNING_THREADS.is_empty() {
        panic!();
    }
}

/*
/// Used to send data from one thread to another.
///
/// NOTE: This will only queue one value at a time, so senders must block for
/// the receiver to finish processing the data.
///
/// Challenges: Should we be able to
pub struct Channel<T> {
    value: UnsafeCell<Option<T>>,
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(None),
        }
    }

    pub async fn send(&'static self, value: T) {
        let v = unsafe { core::mem::transmute::<*mut Option<T>, &mut Option<T>>(self.value.get()) };
        while v.is_some() {
            ChannelChangeFuture {}.await;
        }

        *v = Some(value);
        unsafe { CHANNEL_CHANGED = true };
    }

    pub async fn recv(&'static self) -> T {
        let v = unsafe { core::mem::transmute::<*mut Option<T>, &mut Option<T>>(self.value.get()) };
        loop {
            if let Some(v) = v.take() {
                unsafe { CHANNEL_CHANGED = true };
                return v;
            }

            ChannelChangeFuture {}.await;
        }
    }
}

unsafe impl<T> Sync for Channel<T> {}

struct ChannelChangeFuture {}
impl Future for ChannelChangeFuture {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<()> {
        if unsafe { CHANNEL_CHANGED } {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

struct Select2<T, A: Future<Output = T>, B: Future<Output = T>> {
    a: A,
    b: B,
}

impl<T, A: Future<Output = T> + Unpin, B: Future<Output = T> + Unpin> Future for Select2<T, A, B> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<T> {
        {
            let pinned = Pin::new(&mut self.a);
            if let Poll::Ready(v) = pinned.poll(_cx) {
                return Poll::Ready(v);
            }
        }

        return Poll::Pending;
    }
}

*/

/*
static mut LOCK_CHANGE: bool = false;

pub struct Mutex<T> {
    locked: bool,
    data: T,
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: false,
            data,
        }
    }

    pub async fn lock(&'static self) -> Mut {}
}

pub struct MutexLockFuture {}
*/
