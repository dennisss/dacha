use crate::avr::*;
use core::cell::UnsafeCell;
use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::Poll;

const MAX_NUM_THREADS: usize = 32;

// Runs in a never ending loop while incrementing the value of an integer given
// as an argument.
//
// The counter will be incremented every 16 cycles (or at 1MHz for a 16MHz
// clock). This is meant to run in the main() function will actually operations
// happening in interrupt handlers. This means that a host can retrieve the
// value of the counter at two different points in time to determine the
// utilization.
//
// Based on instruction set timing in:
// https://ww1.microchip.com/downloads/en/DeviceDoc/AVR-Instruction-Set-Manual-DS40002198A.pdf
// using the ATmega32u4 (AVRe+)
//
// Args:
//   addr: *mut u32 : Stored in r24 (low) and r25 (high). This is the address of
//                    integer that should be incremented on each loop cycle.
//
// Internally uses:
// - r18-r21 to store the current value of the counter.
// - Z (r30, r31) to store working address.
// - r22: stores a 0 value.
//
// Never returns.
global_asm!(
    r#"
    .global __idle_loop
__idle_loop:
    ; NOTE: We only use call cloberred registers so don't need to push anything
    ; to the stack

    ; Initialize count to zero
    ; NOTE: We assume that the value at the counter has already been
    ; initialized to 0
    clr r18
    clr r19
    clr r20
    clr r21

    ; r22 will always be zero
    clr r22

__idle_loop_start:
    ; Add 1 to the 32bit counter
    inc r18 ; (1 cycle)
    adc r19, r22 ; += 0 + C (1 cycle)
    adc r20, r22 ; += 0 + C (1 cycle)
    adc r21, r22 ; += 0 + C (1 cycle)

    ; Load Z with the address of the counter (first argument to the function)
    mov r30, r24 ; (1 cycle)
    mov r31, r25 ; (1 cycle)

    ; Store into memory
    st Z+, r18 ; (2 cycles)
    st Z+, r19 ; (2 cycles)
    st Z+, r20 ; (2 cycles)
    st Z+, r21 ; (2 cycles)

    ; Loop
    rjmp __idle_loop_start ; (2 cycles)
"#
);

extern "C" {
    fn __idle_loop(addr: *mut u32);
}

type ThreadFuturePtr = *mut dyn Future<Output = ()>;

struct ThreadVec {
    values: [Option<ThreadFuturePtr>; MAX_NUM_THREADS],
    length: usize,
}

impl ThreadVec {
    const fn new() -> Self {
        Self {
            values: [None; MAX_NUM_THREADS],
            length: 0,
        }
    }

    fn len(&self) -> usize {
        self.length
    }

    /// Pushes the given pointer to the end of the vec.
    fn push(&mut self, ptr: ThreadFuturePtr) {
        self.values[self.length] = Some(ptr);
        self.length += 1;
    }

    fn swap_remove(&mut self, index: usize) {
        assert!(index < self.length);
        if index != self.length - 1 {
            self.values[index] = self.values[self.length - 1].take();
        }
        self.length -= 1;
    }

    fn index(&self, idx: usize) -> ThreadFuturePtr {
        assert!(idx < self.len());
        self.values[idx].unwrap()
    }
}

static mut RUNNING_THREADS: ThreadVec = ThreadVec::new();

static mut THREAD_ID: usize = MAX_NUM_THREADS;

pub struct Thread<Fut: 'static + Sized + Future<Output = ()>> {
    f: &'static dyn Fn() -> Fut,
    fut: Option<Fut>,
    index: usize,
}

impl<Fut: 'static + Sized + Future<Output = ()>> Thread<Fut> {
    pub const fn new(f: &'static dyn Fn() -> Fut) -> Self {
        Self {
            f: f,
            fut: None,
            index: 0,
        }
    }

    pub fn start(&'static mut self) {
        if self.fut.is_some() {
            return;
        }

        self.index = unsafe { RUNNING_THREADS.len() };
        let old_thread_id = unsafe { THREAD_ID };
        unsafe { THREAD_ID = self.index };
        self.fut = Some((self.f)());
        unsafe { THREAD_ID = self.index };

        unsafe {
            RUNNING_THREADS.push(self.fut.as_mut().unwrap() as *mut dyn Future<Output = ()>);
        }
    }

    pub fn stop(&'static mut self) {
        if !self.fut.is_some() {
            return;
        }

        // A thread stopping itself is undefined behavior and should panic.
        assert_ne!(unsafe { THREAD_ID }, self.index);

        self.fut = None;
        unsafe { RUNNING_THREADS.swap_remove(self.index) };
    }
}

#[macro_export]
macro_rules! define_thread {
    ($(#[$meta:meta])* $name: ident, $handler: expr) => {
        $(#[$meta])*
        struct $name {}

        impl $name {
            unsafe fn ptr() -> &'static mut $crate::avr::thread::Thread<impl ::core::future::Future<Output = ()>> {
                type RetType = impl ::core::future::Future<Output = ()>;
                static mut THREAD: $crate::avr::thread::Thread<RetType> = {
                    fn handler_wrap() -> RetType {
                        ($handler)()
                    }
                    $crate::avr::thread::Thread::new(&handler_wrap)
                };
                &mut THREAD
            }

            pub fn start() {
                unsafe { Self::ptr().start() };
            }

            // TODO: If a thread is stopped while one thread is running, we may want to intentionally run an extra cycle to ensure that we re-process them.

            // pub fn stop() {

            // }
        }
    };
}

static mut IDLE_COUNTER: u32 = 0;

pub fn block_on_threads() {
    unsafe {
        disable_interrupts();
        poll_all_threads();
        enable_interrupts();
    }

    unsafe { __idle_loop(&mut IDLE_COUNTER) };

    // NOTE: This should never be reached as the idle_loop should never return.
    panic!();

    // Now sleep (but only if we don't care about performance as sleeping
    // increases wakeup time).
}

/// Attempts to poll every thread once.
///
/// This function is unsafe as it must only be run when interrupts are disabled.
pub unsafe fn poll_all_threads() {
    loop {
        poll_all_threads_inner();

        let new_events: bool = unsafe { CHANNEL_CHANGED };

        if !new_events {
            break;
        }
    }
}

unsafe fn poll_all_threads_inner() {
    let cx = core::mem::transmute::<usize, &mut core::task::Context>(0);

    let mut thread_idx = 0;
    while thread_idx < RUNNING_THREADS.len() {
        let thread_ptr: &mut dyn Future<Output = ()> = {
            core::mem::transmute::<*mut dyn Future<Output = ()>, _>(
                RUNNING_THREADS.index(thread_idx),
            )
        };

        let p = core::pin::Pin::new_unchecked(thread_ptr);
        match core::future::Future::poll(p, cx) {
            core::task::Poll::Ready(()) => {
                RUNNING_THREADS.swap_remove(thread_idx);
                // Re-process the current thread_idx without changes as it *may* have been
                // swapped with the last one
                continue;
            }
            core::task::Poll::Pending => {
                thread_idx += 1;
            }
        }
    }
}

static mut CHANNEL_CHANGED: bool = false;

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
