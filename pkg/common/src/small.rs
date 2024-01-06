// TODO: This code is not yet complete.

use alloc::alloc::{Global, Layout};
use core::alloc::Allocator;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::NonNull;

/// A small vector which stores up to 2 words (16 bytes on a 64-bit system)
/// inline before trying to allocate memory on the heap.
pub struct SmallVec<T> {
    /// Number of elements the vector can store with the currently allocated
    /// memory.
    ///
    /// - If 'capacity * size_of<T> < SmallVecValue<T>', 'value' stores data
    ///   inline and this is the # of elements stored inline.
    /// - Else, this is the total size of the buffer in 'value.ptr'.
    capacity_or_size: usize,

    /// TODO: Make this aligned to 'T'
    value: SmallVecValue<T>,
}

struct SmallVecValue<T> {
    /// Number of elements stored with valid values in the array pointed to by
    /// 'ptr'.
    size: usize,

    ptr: Option<NonNull<T>>,
}

impl<T> SmallVec<T> {
    const INLINE_CAPACITY: usize =
        core::mem::size_of::<SmallVecValue<T>>() / core::mem::size_of::<T>();

    pub fn new() -> Self {
        Self {
            capacity_or_size: 0,
            value: SmallVecValue { size: 0, ptr: None },
        }
    }

    fn get_occupied<'a>(&'a self) -> &'a [T] {
        let (data, capacity, num_valid);

        if self.capacity_or_size <= Self::INLINE_CAPACITY {
            data = unsafe { core::mem::transmute(&self.value) };
            capacity = Self::INLINE_CAPACITY;
            num_valid = self.capacity_or_size;
        } else {
            data = unsafe { self.value.ptr.unwrap_unchecked() }.as_ptr();
            capacity = self.capacity_or_size;
            num_valid = self.value.size;
        }

        unsafe { core::slice::from_raw_parts(data, num_valid) }
    }

    /// Gets the raw segment of memory which spans the entire allocated memory
    /// region and also returns the # of elements that are valid (the rest may
    /// be filled with unitialized memory).
    fn get_allocated_mut<'a>(&'a mut self) -> (&'a mut [MaybeUninit<T>], &'a mut usize) {
        let (data, capacity, num_valid);

        if self.capacity_or_size <= Self::INLINE_CAPACITY {
            data = unsafe { core::mem::transmute(&self.value) };
            capacity = Self::INLINE_CAPACITY;
            num_valid = &mut self.capacity_or_size;
        } else {
            data = unsafe { self.value.ptr.unwrap_unchecked() }.as_ptr();
            capacity = self.capacity_or_size;
            num_valid = &mut self.value.size;
        }

        (
            unsafe { core::slice::from_raw_parts_mut(core::mem::transmute(data), capacity) },
            num_valid,
        )
    }

    fn change_capacity(&mut self, target_capacity: usize) {
        let (current_allocated, num_occupied) = self.get_allocated_mut();

        // Never decrease the capacity.
        if current_allocated.len() >= target_capacity {
            return;
        }

        // Need to copy over elements without drops
        // (ideally just do an optimized mem copy).

        /*
        let layout = Layout::array::<T>(target_capacity).unwrap();

        // let new_allocated =

        // self.value.ptr = Some(Global.allocate(layout).unwrap());
        // self.value.size =

        let mut data = unsafe {
            let ptr = Global.allocate(layout).unwrap();
            Vec::<T>::from_raw_parts(ptr.cast::<T>().as_ptr(), 0, len)
        };
        */
    }

    pub fn push(&mut self, value: T) {
        let (allocated, num_valid) = self.get_allocated_mut();

        if *num_valid < allocated.len() {
            allocated[*num_valid].write(value);
            *num_valid += 1;
            return;
        }

        // Else, need to allocate more memory.
        // In this case,

        // TODO

        /*
        let layout = Layout::array::<T>(len)
            .unwrap()
            .align_to(alignment)
            .unwrap();

        let mut data = unsafe {
            let ptr = Global.allocate(layout).unwrap();
            Vec::<T>::from_raw_parts(ptr.cast::<T>().as_ptr(), 0, len)
        };

        */

        // Global::alloc::alloc::GlobalAlloc::alloc(&self, layout)
    }
}

impl<T> AsRef<[T]> for SmallVec<T> {
    fn as_ref(&self) -> &[T] {
        // unsafe {
        //     let (data, len) = self.get_allocated();
        //     core::mem::transmute(&data[0..len])
        // }

        todo!()
    }
}
