use core::future::Future;
use core::pin::Pin;
use core::ptr::{null, null_mut};
use core::task::Poll;

use crate::stack_pinned::stack_pinned;

// TODO: Remove the Clone and Copy
#[derive(Clone, Copy)]
pub struct WakerList {
    /// Pointer to the first waker in the list or null if the list is empty.
    head: *mut Waker,
}

// The first waker in the list will point back to the 'head' field in the list.
impl !Unpin for WakerList {}

// TODO: Must have a drop which deletes all of the wakers.

impl WakerList {
    pub const fn new() -> Self {
        Self { head: null_mut() }
    }

    /// Adds a waker to this list.
    ///
    /// TODO: Make self Pin as well
    ///
    /// NOTE: It is only valid to call this if the waker is not already in
    /// another list.
    pub fn insert<'a>(&mut self, waker: Pin<&'a mut Waker>) -> Pin<&'a mut Waker> {
        let head = &mut self.head;
        let waker = unsafe { waker.get_unchecked_mut() };
        Self::insert_after(head, waker);
        unsafe { Pin::new_unchecked(waker) }
    }

    fn insert_after(head: &mut *mut Waker, waker: &mut Waker) {
        let old_head = *head;

        // A waker can't be added to multiple lists.
        assert!(waker.prev == null_mut());
        waker.prev = head;
        waker.next = old_head;

        if old_head != null_mut() {
            unsafe { &mut (*old_head) }.prev = &mut waker.next;
        }

        *head = unsafe { core::mem::transmute(waker) };
    }

    /// Wakes up all current wakers in this list by calling their callback
    /// function.
    ///
    /// This will trigger:
    /// - Each Waker instance's Future implementation to return Poll::Ready() on
    ///   the next poll().
    /// - Each waker will be removed from the list.
    ///
    /// NOTE: If a waker is inserted while this is running, it will not be
    /// awakened.
    pub fn wake_all(&mut self) {
        let mut waker_marker = stack_pinned(Waker::new(|_| panic!(), null_mut()));

        let waker_marker = {
            let p = waker_marker.into_pin();
            unsafe { p.get_unchecked_mut() }
        };

        Self::insert_after(&mut self.head, waker_marker);

        // TODO: If there are multiple entries for a single thread, consider marking
        // them all as awakened before invoking the callback.
        while waker_marker.next != null_mut() {
            let next_waker = unsafe { &mut *waker_marker.next };
            next_waker.unlink();
            (next_waker.callback)(next_waker.callback_arg);
        }
    }

    /// Returns whether or not the list contains zero wakers.
    pub fn is_empty(&self) -> bool {
        self.head == null_mut()
    }
}

pub struct Waker {
    /// Pointer to the 'next' pointer of the previous waker (or to the
    /// WakerList::head field if there are no previous wakers).
    ///
    /// May be null if the waker hasn't been added to a waker list.
    prev: *mut *mut Waker,

    next: *mut Waker,

    /// Function to call when we want to wake this waker.
    callback: fn(*mut ()),

    /// Argument to pass to the above callback.
    callback_arg: *mut (),
}

impl !Unpin for Waker {}

impl Drop for Waker {
    fn drop(&mut self) {
        self.unlink()
    }
}

impl Waker {
    pub fn new(callback: fn(*mut ()), callback_arg: *mut ()) -> Self {
        Self {
            prev: null_mut(),
            next: null_mut(),
            callback,
            callback_arg,
        }
    }

    fn unlink(&mut self) {
        if self.prev != null_mut() {
            *unsafe { &mut *self.prev } = self.next;
        }

        if self.next != null_mut() {
            unsafe { &mut *self.next }.prev = self.prev;
        }

        self.prev = null_mut();
        self.next = null_mut();
    }
}

impl Future for Waker {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<()> {
        if self.prev == null_mut() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/*
pub fn test() {
    let mut waker_list = WakerList::new();

    let mut x = Waker {
        prev: null_mut(),
        next: null_mut(),
        callback: || {},
        awakened: false,
    };

    let mut xp = stack_pinned(x);

    {
        let x_pin = xp.into_pin();

        // let xp = unsafe { Pin::new_unchecked(&mut x) };
        // waker_list.append(x_pin);

        // drop(x_pin);
    }

    let y = Box::new(xp);

    // let xp = Pin::new(&mut x);
}
*/

// How do we know that a later is being awakened!

// How do we distinguish between things?
