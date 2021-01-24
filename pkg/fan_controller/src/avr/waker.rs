/// This module contains the Waker data type which can be used to
use crate::avr::arena_stack::*;
use crate::avr::thread::*;
use core::future::Future;
use core::pin::Pin;
use core::task::Context;
use core::task::Poll;

use crate::avr::serial::*;

// NOTE: Can be at most 'ArenaIndex::MAX_VALUE + 1'
const MAX_PENDING_WAKERS: usize = 4;

static mut PENDING_WAKERS: [ArenaStackItem<Waker>; MAX_PENDING_WAKERS] =
    [ArenaStackItem::empty(Waker { thread: 0 }); MAX_PENDING_WAKERS];

/// List of all unused entries in the arena.
static mut FREE_LIST: ArenaStack<Waker, WakerArena> = ArenaStack::new(WakerArena::new());
static mut FREE_LIST_INITIALIZED: bool = false;

static mut CURRENT_BEING_AWAKEN: Option<ArenaIndex> = None;

/// Initializes the PENDING_WAKERS array.
///
/// This assigns all slots in the arena to be initially part of the free list.
///
/// NOTE: This MUST be called before using any Waker related functions. This
/// should only be called within avr::thread::block_on_threads().
pub fn init() {
    // Don't initialize if already initialized (mainly for use in tests).
    if (unsafe { FREE_LIST_INITIALIZED }) {
        return;
    }

    for i in 0..MAX_PENDING_WAKERS {
        unsafe { FREE_LIST.push(i as ArenaIndex, Waker { thread: 0 }) };
    }

    unsafe { FREE_LIST_INITIALIZED = true };
}

#[derive(Clone, Copy)]
struct Waker {
    thread: ThreadId,
}

// TODO: Remove the clone/copy
#[derive(Clone, Copy)]
pub struct WakerList {
    inner: ArenaStack<Waker, WakerArena>,
}

impl WakerList {
    pub const fn new() -> Self {
        Self {
            inner: ArenaStack::new(WakerArena::new()),
        }
    }

    #[no_mangle]
    #[inline(never)]
    pub fn add(self: &'static mut WakerList) -> WakerFuture {
        // uart_send_sync(b"TRY ALLOC \n");

        let thread = current_thread_id();
        let index = WakerArena::alloc();
        self.inner.push(index, Waker { thread });

        uart_send_sync(b"ALLOC WAKER ");
        uart_send_number_sync(index);
        uart_send_sync(b"\n");

        WakerFuture {
            list: self,
            id: Some(index),
        }
    }

    /// NOTE: This will only wake up all wakers already in the list. Any new
    /// wakers added after this function starts will not be awaken.
    #[no_mangle]
    #[inline(never)]
    pub fn wake_all(self: &mut WakerList) {
        uart_send_sync(b"WAKE ALL\n\n");

        let mut cur_waker = self.inner.peek();

        while let Some((waker, index)) = cur_waker.take() {
            unsafe {
                assert!(CURRENT_BEING_AWAKEN.is_none());
                CURRENT_BEING_AWAKEN = Some(index);

                uart_send_sync(b"WAKING WAKER ");
                uart_send_number_sync(index);
                uart_send_sync(b"\n");

                crate::avr::thread::poll_thread(waker.thread);

                CURRENT_BEING_AWAKEN = None;
            }

            // NOTE: This must be after the polling to ensure that we don't
            // immediately create a waker with the same id as the
            // one being actively woken up.
            //
            // Also
            cur_waker = self.inner.remove(index);
            WakerArena::free(index);

            // uart_send_sync(b"FREE WAKER ");
            // uart_send_number_sync(index);
            // uart_send_sync(b"\n");
        }
    }
}

// impl Copy for WakerList {}

// impl Clone for WakerList {
//     fn clone(&self) -> Self {
//         assert!(self.inner.is_empty());
//         Self::new()
//     }
// }

// TODO: Remove the clone/copy
#[derive(Clone, Copy)]
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
        unsafe { FREE_LIST.push(index, Waker { thread: 0 }) };
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
    pub id: Option<ArenaIndex>,
}

impl Future for WakerFuture {
    type Output = ();

    #[inline(always)]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<()> {
        assert!(self.id.is_some());
        if unsafe { CURRENT_BEING_AWAKEN } == self.id {
            uart_send_sync(b"WAKER READY\n");

            // NOTE: The underlying waker will be freed in wake_all after the thread is done
            // running.
            self.id = None;
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
            // NOTE: We will never remove an id that is actively being looked at by
            // wake_all().
            if unsafe { CURRENT_BEING_AWAKEN } != self.id {
                uart_send_sync(b"DROP WAKER ");
                uart_send_number_sync(id);
                uart_send_sync(b"\n");

                self.list.inner.remove(id);
                WakerArena::free(id);
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
