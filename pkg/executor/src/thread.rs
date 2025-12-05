use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::raw_waker::RAW_WAKER;
use crate::CriticalSection;

/// Reference to the thread's polling function
///
/// TODO: Should be possible to optimize this down to only a single pointer to a
/// 'fn() -> Poll<()>'
#[derive(Clone, Copy)]
struct ThreadReference {
    /// Type erased thread instance pointer (a '*mut Thread<Fut>').
    ptr: *mut (),

    /// Function which can be passed the above 'ptr' to poll/wake the thread.
    poll_fn: fn(*mut ()),
}

static mut CURRENT_THREAD: Option<ThreadReference> = None;

///
pub struct Thread<Fut: 'static + Sized + Future<Output = ()>> {
    // TODO: Should technically use a mutex for this?
    fut: Option<Fut>,
    polling: bool,
}

impl<Fut: 'static + Sized + Future<Output = ()>> Thread<Fut> {
    pub const fn new() -> Self {
        Self { fut: None, polling: false }
    }

    #[inline(always)]
    pub fn is_running(&self) -> bool {
        let cs = CriticalSection::new();
        self.fut.is_some()
    }

    #[inline(always)]
    pub fn start<F: FnOnce() -> Fut>(&'static mut self, f: F) {
        // TODO: Validate not restarting inside of our own thread.

        let cs = CriticalSection::new();
        
        if self.polling {
            return;
        }
        
        // TODO: Call stop() to handle stuff.
        // Clean up the past run of this thread.
        self.fut = None;

        self.fut = Some(f());

        Self::poll_inner(unsafe { core::mem::transmute(&mut *self) }, cs);;
    }

    #[inline(never)]
    fn poll(ptr: *mut ()) {
        Self::poll_inner(ptr, CriticalSection::new());
    }

    fn poll_inner(ptr: *mut (), cs: CriticalSection) {
        let this: &mut Self = unsafe { core::mem::transmute(ptr) };

        if this.polling {
            return;
        }

        this.polling = true;

        // static waker: Waker = unsafe { Waker::from_raw(RAW_WAKER) };
        // static mut cx: Context = Context::from_waker(&waker);

        // TODO: Does this waker have to live for the netier life?
        let waker = unsafe { Waker::from_raw(RAW_WAKER) };
        let mut cx = Context::from_waker(&waker);
        let p = unsafe { Pin::new_unchecked(this.fut.as_mut().unwrap()) };

        let parent_thread = unsafe { CURRENT_THREAD.take() };

        unsafe {
            CURRENT_THREAD = Some(ThreadReference {
                ptr,
                poll_fn: Self::poll,
            })
        };

        drop(cs);

        let res = p.poll(&mut cx);

        let cs = CriticalSection::new();

        this.polling = false;

        match res {
            Poll::Ready(()) => {
                this.fut = None;
            }
            Poll::Pending => {}
        }

        unsafe {
            CURRENT_THREAD = parent_thread;
        }

        drop(cs);
    }

    /// NOTE: Currently if the thread is actively being polled, then this will do nothing.
    pub fn stop(&'static mut self) {
        let cs = CriticalSection::new();

        if self.polling {
            // TODO: Implement asking for a stop after the poll is done.
            return;
        }

        if !self.fut.is_some() {
            return;
        }

        // Drop all variables. This should also drop any WakerFutures used by the thread
        // (thus ensuring that this thread id is safe to re-use later).
        self.fut = None;

        drop(cs);
    }
}

/// Since this touches CURRENT_THREAD which is a 'static mut', this must be run uninterrupted. 
pub fn new_waker_for_current_thread(cs: &mut CriticalSection) -> crate::waker::Waker {
    let current_ref = unsafe { CURRENT_THREAD.as_ref().unwrap() };
    crate::waker::Waker::new(current_ref.poll_fn, current_ref.ptr)
}

// Must return a stack pinned value!
// pub fn spawn<F: Future<Output = ()>>(f: F) {
//     static mut THREAD: Thread = Thread::new();
//     unsafe { THREAD.start(move || f) };
// }

#[macro_export]
macro_rules! define_thread {
    ($(#[$meta:meta])* $name: ident, $handler: ident $(, $arg:ident : $t:ty )*) => {
        $(#[$meta])*
        pub struct $name {}

        const _: () = {
            trait ThreadFn {
                // TODO: Require futures to be Send?
                type Fut: ::core::future::Future<Output = ()> + 'static;
                fn start($($arg: $t,)*) -> Self::Fut;
            }

            impl ThreadFn for () {
                type Fut = impl ::core::future::Future<Output = ()> + 'static;

                fn start($($arg: $t,)*) -> Self::Fut {
                    $handler($($arg,)*)
                }
            }


            type ThreadFnFut = <() as ThreadFn>::Fut;

            static mut THREAD: $crate::thread::Thread<ThreadFnFut> = {
                $crate::thread::Thread::new()
            };

            impl $name {
                /// Starts executing the thread (immediately context switching to running the first poll() cycle of it).
                ///
                /// If the thread is already running, then the old future is destroyed before newly starting the thread.
                pub fn start($($arg: $t,)*) {
                    unsafe { THREAD.start(move || -> ThreadFnFut { <() as ThreadFn>::start($($arg,)*) }) };
                }

                pub fn stop() {
                    unsafe { THREAD.stop() };
                }

                pub fn is_running() -> bool {
                    unsafe { THREAD.is_running() }
                }
            }
        };
    };
}
