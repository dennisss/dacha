use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::raw_waker::RAW_WAKER;

/// Reference to the thread's polling function
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
    fut: Option<Fut>,
}

impl<Fut: 'static + Sized + Future<Output = ()>> Thread<Fut> {
    pub const fn new() -> Self {
        Self { fut: None }
    }

    #[inline(always)]
    pub fn start<F: FnOnce() -> Fut>(&'static mut self, f: F) {
        // TODO: Validate not restarting inside of our own thread.

        // Clean up the past run of this thread.
        self.fut = None;

        self.fut = Some(f());

        Self::poll(unsafe { core::mem::transmute(&mut *self) });
    }

    #[inline(never)]
    fn poll(ptr: *mut ()) {
        let this: &mut Self = unsafe { core::mem::transmute(ptr) };

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

        match p.poll(&mut cx) {
            Poll::Ready(()) => {
                this.fut = None;
            }
            Poll::Pending => {}
        }

        unsafe {
            CURRENT_THREAD = parent_thread;
        }
    }

    pub fn stop(&'static mut self) {
        if !self.fut.is_some() {
            return;
        }

        // TODO: Assert not stopping ourselves.

        // A thread stopping itself is undefined behavior and should panic.
        // avr_assert_ne!(unsafe { CURRENT_THREAD_ID }, Some(self.id));

        // Drop all variables. This should also drop any WakerFutures used by the thread
        // (thus ensuring that this thread id is safe to re-use later).
        self.fut = None;

        // unsafe { RUNNING_THREADS.remove(self.id) };
    }
}

pub fn new_waker_for_current_thread() -> crate::waker::Waker {
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
                pub fn start($($arg: $t,)*) {
                    unsafe { THREAD.start(move || -> ThreadFnFut { <() as ThreadFn>::start($($arg,)*) }) };
                }

                pub fn stop() {
                    unsafe { THREAD.stop() };
                }
            }
        };
    };
}
