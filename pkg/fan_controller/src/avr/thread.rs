use crate::avr::*;
use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::Poll;
use crate::{avr_assert, avr_assert_ne};

// 1398
// 1454 - 687
pub const MAX_NUM_THREADS: usize = 16;

// TODO: Implement support for sleeping
// (e.g. when the computer is off or not receiving any USB traffic).

/// Type used to represent the id of a thread.
///
/// This is also the index of the thread into the internal RUNNING_THREADS
/// array.
///
/// NOTE: This implies that we can have up to 256 threads.
pub type ThreadId = u8;

/// Reference to the thread's polling function
/// TODO: Should be possible to optimize this down to only a single pointer to a
/// 'fn() -> Poll<()>'
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

    #[inline(never)]
    fn get(&self, index: ThreadId) -> &ThreadReference {
        if let Some(value) = self.values.get(index as usize) {
            if let Some(thread) = value {
                return thread;
            }  
        }

        panic!();
    }

    #[inline(never)]
    fn push(&mut self, value: ThreadReference) -> ThreadId {
        for (i, item) in self.values.iter_mut().enumerate() {
            if item.is_none() {
                *item = Some(value);
                self.length += 1;
                return i as ThreadId;
            }
        }

        panic!();
    }

    #[inline(never)]
    fn remove(&mut self, index: ThreadId) {
        // assert!(self.values[index as usize].take().is_some());
        self.values[index as usize] = None;
        self.length -= 1;
    }

    #[inline(never)]
    fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[inline(never)]
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
        // if self.fut.is_some() {
        //     return;
        // }

        // tODO: Validate not restarting inside of our own thread.

        let restarting = self.fut.is_some();

        // self.fut = None;

        self.fut = Some(f());

        // TODO: This assumes that the thread didn't previosuly terminate.
        if !restarting {
            self.id = unsafe {
                RUNNING_THREADS.push(ThreadReference {
                    poll_fn: Self::poll_future,
                    future: core::mem::transmute(self.fut.as_mut().unwrap()),
                })
            };
        }

        unsafe {
            // If the executor is already running, schedule the initial run of this thread
            // for the next cycle. NOTE: We don't have directly calling
            // poll_thread here as would require having possibly large nested stacks.
            if THREADS_INITIALIZED {
                crate::avr::interrupts::wake_up_thread_by_id(self.id);
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
        p.poll(&mut cx)
    }

    pub fn stop(&'static mut self) {
        if !self.fut.is_some() {
            return;
        }

        // A thread stopping itself is undefined behavior and should panic.
        avr_assert_ne!(unsafe { CURRENT_THREAD_ID }, Some(self.id));

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
            /* -> &'static mut $crate::avr::thread::Thread<impl ::core::future::Future<Output=()>> */
            fn ptr() -> (fn(), fn()) {
                type RetType = impl ::core::future::Future<Output = ()>;
                #[inline(always)]
                fn handler_wrap() -> RetType {
                    ($handler)()
                }

                static mut THREAD: $crate::avr::thread::Thread<RetType> = {
                    $crate::avr::thread::Thread::new()
                };

                fn start() {
                    unsafe { THREAD.start(handler_wrap) };
                }

                fn stop() {
                    unsafe { THREAD.stop() };
                }

                (start, stop)

                // unsafe { THREAD.start(handler_wrap) };

                // unsafe { &mut THREAD }
            }

            #[inline(always)]
            pub fn start() {
                (Self::ptr().0)();
            }

            // TODO: If a thread is stopped while one thread is running, we may want to intentionally run an extra cycle to ensure that we re-process them.

            // TODO: Ensure that this doesn't first restart the thread.
            pub fn stop() {
                (Self::ptr().1)();
                // thread.stop();
            }
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
    unsafe { crate::avr::interrupts::init() };

    // {
    //     // NOTE: Stack is from 256 - 2815.
    //     let x: u8 = 0;
    //     let sp: u16 = unsafe { core::mem::transmute(&x) };
    //     crate::usart::USART1::send_blocking(b"Stack: ");
    //     crate::debug::num_to_slice16(sp, |s| {
    //         crate::usart::USART1::send_blocking(s);
    //     });
    //     crate::usart::USART1::send_blocking(b"\n");

    //     let xp: u16 = unsafe { core::mem::transmute(&THREADS_INITIALIZED) };
    //     crate::usart::USART1::send_blocking(b"Threads: ");
    //     crate::debug::num_to_slice16(xp, |s| {
    //         crate::usart::USART1::send_blocking(s);
    //     });
    //     crate::usart::USART1::send_blocking(b"\n");
    // }

    unsafe {
        THREADS_INITIALIZED = true;

        // Poll all threads for the first time so that wakers can be initialized.
        // TODO: Verify that this works even if the threads start more threads.
        for (id, _thread) in RUNNING_THREADS.iter() {
            unsafe { poll_thread(id) };
        }

        // Usually to be called by the hardware interrupt handlers, but we aren't in
        // that context yet.
        crate::avr::interrupts::wake_all_internal();

        enable_interrupts();

        crate::avr::subroutines::avr_idle_loop(&mut IDLE_COUNTER);
    }

    // NOTE: This should never be reached as the idle_loop should never return.
    panic!();
}

use core::task::{Context, RawWaker, RawWakerVTable, Waker};

unsafe fn raw_waker_clone(data: *const ()) -> RawWaker {
    RAW_WAKER
}

unsafe fn raw_waker_wake(data: *const ()) {}

unsafe fn raw_waker_wake_by_ref(data: *const ()) {}

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
    avr_assert!(CURRENT_THREAD_ID.is_none());
    CURRENT_THREAD_ID = Some(thread_id);

    let result = (thread_ref.poll_fn)(thread_ref.future);

    CURRENT_THREAD_ID = None;

    if result.is_ready() {
        // TODO: Also set the future to None in the thread instance to ensure that everything is dropped?
        // TODO: Must also ensure that all events are cleaned up
        // crate::USART1::send_blocking(b"THREAD READY!\n");
        // panic!();
        RUNNING_THREADS.remove(thread_id);
        
    }

    if RUNNING_THREADS.is_empty() {
        // TODO: Use AVR panic
        panic!();
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
