#![feature(const_fn_trait_bound, negative_impls, type_alias_impl_trait, asm)]
#![no_std]

#[cfg(feature = "std")]
extern crate common;
#[cfg(feature = "std")]
extern crate std;

extern crate peripherals_raw;

pub mod arena_stack;
pub mod futures;
mod raw_waker;
pub mod singleton;
pub mod stack_pinned;
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
