#![feature(
    const_fn_trait_bound,
    negative_impls,
    type_alias_impl_trait,
    impl_trait_in_assoc_type,
    asm,
    waker_getters,
    thread_local
)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate macros;
#[cfg(feature = "std")]
#[macro_use]
extern crate common;

pub mod arena_stack;
#[cfg(feature = "std")]
pub mod bundle;
#[cfg(feature = "std")]
pub mod cancellation;
#[cfg(feature = "std")]
pub mod channel;
#[cfg(feature = "std")]
pub mod child_task;
#[cfg(feature = "std")]
pub mod future;
pub mod futures;
#[cfg(feature = "std")]
pub mod loop_throttler;
mod raw_waker;
#[cfg(feature = "std")]
pub mod signals;
pub mod singleton;
pub mod stack_pinned;
pub mod sync;
pub mod thread;
pub mod waker;

#[cfg(target_label = "cortex_m")]
mod cortex_m;

#[cfg(target_label = "cortex_m")]
pub use cortex_m::*;

#[cfg(feature = "std")]
pub mod linux;

#[cfg(feature = "std")]
pub use linux::*;

#[cfg(test)]
mod tests {
    use core::ptr::null_mut;
    use std::println;

    use super::*;

    static mut WAKER_LIST: waker::WakerList = waker::WakerList::new();

    define_thread!(TestThread, TestThreadFn);
    async fn TestThreadFn() {
        println!("ONE");

        wait_once().await;

        println!("TWO");

        wait_once().await;

        println!("THREE");
    }

    define_thread!(TestThread2, TestThread2Fn);
    async fn TestThread2Fn() {
        println!("2 ONE");

        wait_once().await;

        println!("2 TWO");

        wait_once().await;

        println!("2 THREE");
    }

    async fn wait_once() {
        let mut waker =
            crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread());

        let waker = unsafe { WAKER_LIST.insert(waker.into_pin()) };

        waker.await;
    }

    #[test]
    fn run_wakers() {
        let starter = std::thread::spawn(|| {
            TestThread::start();
        });

        let starter2 = std::thread::spawn(|| {
            TestThread2::start();
        });

        std::thread::sleep(std::time::Duration::from_secs(1));

        unsafe { WAKER_LIST.wake_all() };

        std::thread::sleep(std::time::Duration::from_secs(1));

        unsafe { WAKER_LIST.wake_all() };

        std::thread::sleep(std::time::Duration::from_secs(1));

        starter.join();
    }
}
