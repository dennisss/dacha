// Small allocator implementation so that we can selectively allocate a few objects on the
// heap while keeping the majority of stuff alloc free.
// 
// In the context of the PeripheralsController, assuming allocations are only make during
// peripheral configuration, all memory should be reclaimed when all peripherals are
// unconfigured (num_allocated will reach 0 in the bump allocator below).

use core::cell::UnsafeCell;
use core::mem::{transmute, MaybeUninit, size_of};
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use core::convert::{AsRef, AsMut};

extern "C" {
    static mut _sheap: u8;
}

static mut ALLOCATOR: BumpAllocator = BumpAllocator {
    next: unsafe { transmute(&_sheap) },
    num_allocated: 0
};

struct BumpAllocator {
    next: *mut u8,
    num_allocated: usize 
}

impl BumpAllocator {
    fn alloc(&mut self, mut size: usize) -> *mut u8 {
        let ptr = self.next;

        // Always align the size to 32-bit for simplicity.
        size = size.next_multiple_of(4);

        self.next = unsafe { self.next.add(size) };
        self.num_allocated += 1;
        ptr
    }

    fn dealloc(&mut self) {
        self.num_allocated -= 1;

        if self.num_allocated == 0 {
            self.next = unsafe { transmute(&_sheap) };
        }
    }
}

#[repr(transparent)]
pub struct Box<T> {
    ptr: *mut T,
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            core::ptr::drop_in_place(self.ptr);
            ALLOCATOR.dealloc();
        }
    }
}

impl<T: Default> Default for Box<T> {
    fn default() -> Self {
        unsafe { 
            let ptr = ALLOCATOR.alloc(size_of::<T>()) as *mut T;
            core::ptr::write(ptr, T::default());
            Self { ptr }
        }
    }
}

unsafe impl<T: Sync> Sync for Box<T> {}
unsafe impl<T: Send> Send for Box<T> {}


pub struct BoxedSlice<T> {
    len: usize,
    data: Box<T>,
}

pub trait Primitive {}

impl Primitive for u8 {}
impl Primitive for i16 {}


// NOTE: new_zeroed is only safe for basic types which make sense being zero'ed in memory.
impl<T: Sized + Primitive> BoxedSlice<T> {

    pub fn new_zeroed(len: usize) -> Self {
        unsafe {
            let size = size_of::<T>() * len;

            // TODO: THis will always be 4 byte aligned so should be fast to zero.
            let ptr = ALLOCATOR.alloc(size);
            core::ptr::write_bytes(ptr, 0x00, size);

            BoxedSlice {
                len,
                data: Box {
                    ptr: ptr as *mut T
                }
            }
        }
    }
}

impl<T> Deref for BoxedSlice<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe {
            core::slice::from_raw_parts(self.data.ptr, self.len)
        }
    }
}

impl<T> DerefMut for BoxedSlice<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            core::slice::from_raw_parts_mut(self.data.ptr, self.len)
        }
    }
}

impl<T> AsRef<[T]> for BoxedSlice<T> {
    fn as_ref(&self) -> &[T] {
        &*self
    }
}

impl<T> AsMut<[T]> for BoxedSlice<T> {
    fn as_mut(&mut self) -> &mut [T] {
        &mut *self
    }
}

unsafe impl<T: Sync> Sync for BoxedSlice<T> {}
unsafe impl<T: Send> Send for BoxedSlice<T> {}


