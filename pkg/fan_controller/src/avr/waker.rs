/// This module contains the Waker data type which can be used to
use core::future::Future;
use core::pin::Pin;
use core::task::Context;
use core::task::Poll;

use executor::avr::arena_stack::*;

use crate::avr::thread::*;
use crate::avr_assert;

// NOTE: Can be at most 'ArenaIndex::MAX_VALUE + 1'
const MAX_PENDING_WAKERS: usize = 16;

const INVALID_THREAD_ID: ThreadId = 255;

/// Arena memory which stores
static mut PENDING_WAKERS: [ArenaStackItem<Waker>; MAX_PENDING_WAKERS] =
    [ArenaStackItem::empty(Waker {
        thread: INVALID_THREAD_ID,
        ref_count: 0,
    }); MAX_PENDING_WAKERS];

/// List of all unused entries in the arena.
static mut FREE_LIST: ArenaStack<Waker, WakerArena> = ArenaStack::new(WakerArena::new());

/// Whether or not FREE_LIST has been initialized. Set by the first call to
/// init().
static mut FREE_LIST_INITIALIZED: bool = false;

static mut CURRENT_BEING_AWAKEN: Option<ArenaIndex> = None;

/// Initializes the PENDING_WAKERS array.
///
/// This assigns all slots in the arena to be initially part of the free list.
///
/// NOTE: This MUST be called before using any Waker related functions. This
/// should only be called within avr::thread::block_on_threads().
pub fn init() {
    // Don't initialize if already initialized (mainly for use in unit tests).
    // TODO: On AVR, assert that it isn't already set.
    if (unsafe { FREE_LIST_INITIALIZED }) {
        return;
    }

    // Initially every single element in the PENDING_WAKERS list is free.
    for i in 0..MAX_PENDING_WAKERS {
        unsafe {
            FREE_LIST.push(
                i as ArenaIndex,
                Waker {
                    thread: INVALID_THREAD_ID,
                    ref_count: 0,
                },
            )
        };
    }

    unsafe { FREE_LIST_INITIALIZED = true };
}

#[derive(Clone, Copy, PartialEq)]
struct Waker {
    thread: ThreadId,
    ref_count: u8,
}

pub struct WakerList {
    inner: ArenaStack<Waker, WakerArena>,
}

impl WakerList {
    pub const fn new() -> Self {
        Self {
            inner: ArenaStack::new(WakerArena::new()),
        }
    }

    fn incref(&mut self, index: ArenaIndex) {
        self.inner.update(index, |waker| {
            waker.ref_count += 1;
        });
    }

    fn deref_leaking(&mut self, index: ArenaIndex) -> u8 {
        let mut new_ref_count = 0;
        self.inner.update(index, |waker| {
            waker.ref_count -= 1;
            new_ref_count = waker.ref_count;
        });
        new_ref_count
    }

    fn deref(&mut self, index: ArenaIndex) {
        if self.deref_leaking(index) == 0 {
            self.inner.remove(index);
            WakerArena::free(index);
        }
    }

    #[no_mangle]
    #[inline(never)]
    pub fn add(&'static mut self) -> WakerFuture {
        let thread = current_thread_id();
        self.add_for_thread(thread)
    }

    // TODO: The dropping of the future will kill the waker?
    #[no_mangle]
    #[inline(never)]
    pub fn add_for_thread(&'static mut self, thread: ThreadId) -> WakerFuture {
        let index = WakerArena::alloc();

        self.inner.push(
            index,
            Waker {
                thread,
                ref_count: 1,
            },
        );

        // TODO: This is returned very inefficiently.
        WakerFuture {
            list: self,
            id: Some(index),
        }
    }

    /*
        Challenges:
        - If wake_all performs the removal of the items, then I can't stop threads because that would requie
        - If an item is currently being awoken, then no need for the waker to delete it because

    */

    // There are a few scenarios:
    // - Waking without caring about it being dropped (for the sake of starting a
    //   thread)
    // - Waking and

    // All wakers start with ref_count of 2
    // - When it is dropped, that is allowed to completely remove it assuming that
    //   is isn't currently being awoken.
    // - When it seen in wake_all, wake_all will temporarily increment by 1 the
    //   reference,

    /// NOTE: This will only wake up all wakers already in the list. Any new
    /// wakers added after this function starts will not be awaken.
    pub fn wake_all(&'static mut self) {
        let mut cur_waker: Option<(Waker, ArenaIndex)> = self.inner.peek();

        while let Some((waker, index)) = cur_waker.take() {
            self.incref(index);

            unsafe {
                avr_assert!(CURRENT_BEING_AWAKEN.is_none());
                CURRENT_BEING_AWAKEN = Some(index);
                crate::avr::thread::poll_thread(waker.thread);
                CURRENT_BEING_AWAKEN = None;
            }

            cur_waker = self.inner.before(index);

            self.deref(index);

            // TODO: When using a leaked future, this is totally feasible.
            // if !(unsafe { CURRENT_WAS_AWAKENED }) {
            //     crate::usart::USART1::send_blocking(b"NO WAKE ");
            //     crate::avr::debug::num_to_slice(waker.thread, |s| {
            //         crate::usart::USART1::send_blocking(s);
            //     });
            //     crate::usart::USART1::send_blocking(b"\n");
            // }

            // TODO: What if we remove it from the list but the Waker instance
            // wasn't destroyed? ^ This would mean that it would
            // come back later SO, we can only remove the waker here
            // if the WakerFuture was called.

            // But, when wouldn't it be called (if waking the thread)?
            //

            // NOTE: This must be after the polling to ensure that we don't
            // immediately create a waker with the same id as the
            // one being actively woken up.
            //
            // Also
            // crate::usart::USART1::send_blocking(b"<\n");
            // cur_waker = self.inner.remove(index);

            // if new_cur_waker != cur_waker {
            //     crate::usart::USART1::send_blocking(b">\n");
            // }

            // crate::usart::USART1::send_blocking(b">\n");
            // WakerArena::free(index);

            // cur_waker = new_cur_waker;
        }
    }
}

struct WakerArena {}

impl WakerArena {
    const fn new() -> Self {
        Self {}
    }
}

impl WakerArena {
    fn alloc() -> ArenaIndex {
        unsafe { FREE_LIST.pop() }.unwrap().1
    }

    fn free(index: ArenaIndex) {
        unsafe {
            FREE_LIST.push(
                index,
                Waker {
                    thread: INVALID_THREAD_ID,
                    ref_count: 0,
                },
            )
        };
    }
}

impl Arena<ArenaStackItem<Waker>> for WakerArena {
    #[inline(always)]
    fn get(&self, index: ArenaIndex) -> ArenaStackItem<Waker> {
        unsafe { PENDING_WAKERS[index as usize] }
    }

    #[inline(always)]
    fn set(&self, index: ArenaIndex, value: ArenaStackItem<Waker>) {
        unsafe { PENDING_WAKERS[index as usize] = value };
    }
}

pub struct WakerFuture {
    list: &'static mut WakerList,
    id: Option<ArenaIndex>,
}

impl WakerFuture {
    pub unsafe fn leak_waker(mut self) {
        self.list.deref_leaking(self.id.take().unwrap());
    }
}

impl Future for WakerFuture {
    type Output = ();

    #[inline(never)]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<()> {
        avr_assert!(self.id.is_some());
        if unsafe { CURRENT_BEING_AWAKEN == self.id } {
            // NOTE: The underlying waker will be freed in wake_all after the thread is done
            // running.
            let id = self.id.take().unwrap();
            self.list.deref(id);
            // self.id = None;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for WakerFuture {
    fn drop(&mut self) {
        // TODO: Instead add to a 'pending deletion' list so that we don't
        // re-use in the same run.
        if let Some(id) = self.id {
            self.list.deref(id);

            // NOTE: We will never remove an id that is actively being looked at by
            // wake_all().
            if unsafe { CURRENT_BEING_AWAKEN != self.id } {
                // self.list.inner.remove(id);
                // WakerArena::free(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::define_thread;
    use std::cell::Cell;

    static mut WAKER_LIST: WakerList = WakerList::new();

    static mut COUNTER: usize = 0;

    define_thread!(TestThread, test_thread);
    async fn test_thread() {
        loop {
            unsafe {
                COUNTER += 1;
                WAKER_LIST.add().await;
                COUNTER += 10;
                WAKER_LIST.add().await;
            }
        }
    }

    fn counter() -> usize {
        unsafe { COUNTER }
    }

    fn wake_all() {
        unsafe { WAKER_LIST.wake_all() };
    }

    #[test]
    fn can_wake_a_thread() {
        TestThread::start();
        assert_eq!(counter(), 0);

        init();

        // We haven't polled the thread yet, so no wakers are registered.
        wake_all();
        assert_eq!(counter(), 0);

        unsafe { poll_thread(0) };
        assert_eq!(counter(), 1);

        wake_all();
        assert_eq!(counter(), 11);

        for i in 0..10000 {
            unsafe { poll_thread(0) };
            assert_eq!(counter(), 11);
        }

        wake_all();
        assert_eq!(counter(), 12);

        let initial_value = 12;
        for i in 0..100 {
            wake_all();
            assert_eq!(counter(), initial_value + i * 11 + 10);
            wake_all();
            assert_eq!(counter(), initial_value + i * 11 + 11);
        }

        TestThread::stop();
    }
}
