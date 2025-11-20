// Small allocator implementation so that we can selectively allocate a few objects on the
// heap while keeping the majority of stuff alloc free.
// 
// In the context of the PeripheralsController, assuming allocations are only make during
// peripheral configuration, all memory should be reclaimed when all peripherals are
// unconfigured (num_allocated will reach 0 in the bump allocator below).

use core::cell::UnsafeCell;
use core::mem::transmute;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

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
            let ptr = ALLOCATOR.alloc(core::mem::size_of::<T>()) as *mut T;
            core::ptr::write(ptr, T::default());
            Self { ptr }
        }
    }
}

unsafe impl<T: Sync> Sync for Box<T> {}
unsafe impl<T: Send> Send for Box<T> {}
