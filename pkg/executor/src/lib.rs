#![feature(const_fn_trait_bound, negative_impls, type_alias_impl_trait)]
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod arena_stack;
pub mod stack_pinned;
pub mod thread;
pub mod waker;

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

    async fn wait_once() {
        let mut waker =
            crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread());

        let waker = unsafe { waker.into_pin().get_unchecked_mut() };

        unsafe {
            WAKER_LIST.insert(waker);
        }

        unsafe { core::pin::Pin::new_unchecked(waker) }.await;
    }

    #[test]
    fn run_wakers() {
        let starter = std::thread::spawn(|| {
            TestThread::start();
        });

        std::thread::sleep(std::time::Duration::from_secs(1));

        unsafe { WAKER_LIST.wake_all() };

        std::thread::sleep(std::time::Duration::from_secs(1));

        unsafe { WAKER_LIST.wake_all() };

        std::thread::sleep(std::time::Duration::from_secs(1));

        starter.join();
    }
}
