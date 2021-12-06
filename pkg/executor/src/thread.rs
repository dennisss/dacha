use core::future::Future;
use core::iter::Iterator;
use core::pin::Pin;
use core::task::Poll;

/// Reference to the thread's polling function
/// TODO: Should be possible to optimize this down to only a single pointer to a
/// 'fn() -> Poll<()>'
#[derive(Clone, Copy)]
struct ThreadReference {
    /// Type erased Future<Output=()> which contains the state of the thread.
    ptr: *mut (),

    /// Function which can be passed the above 'future' to poll/wake the thread.
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
    pub fn start(&'static mut self, f: fn() -> Fut) {
        // TODO: Validate not restarting inside of our own thread.

        // Clean up the past run of this thread.
        self.fut = None;

        self.fut = Some(f());

        Self::poll(unsafe { core::mem::transmute(&mut *self) });
    }

    fn poll(ptr: *mut ()) {
        let this: &mut Self = unsafe { core::mem::transmute(ptr) };

        // static waker: Waker = unsafe { Waker::from_raw(RAW_WAKER) };
        // static mut cx: Context = Context::from_waker(&waker);

        // TODO: Does this waker have to live for the netier life?
        let waker = unsafe { Waker::from_raw(RAW_WAKER) };
        let mut cx = Context::from_waker(&waker);
        let p = unsafe { Pin::new_unchecked(this.fut.as_mut().unwrap()) };

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
            CURRENT_THREAD = None;
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

// pub fn current_thread_id() -> ThreadId {
//     unsafe { CURRENT_THREAD_ID.unwrap() }
// }

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

                static mut THREAD: $crate::thread::Thread<RetType> = {
                    $crate::thread::Thread::new()
                };

                fn start() {
                    unsafe { THREAD.start(handler_wrap) };
                }

                fn stop() {
                    unsafe { THREAD.stop() };
                }

                (start, stop)
            }

            #[inline(always)]
            pub fn start() {
                (Self::ptr().0)();
            }

            // TODO: If a thread is stopped while one thread is running, we may want to intentionally run an extra cycle to ensure that we re-process them.

            // TODO: Ensure that this doesn't first restart the thread.
            pub fn stop() {
                (Self::ptr().1)();
            }
        }
    };
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
