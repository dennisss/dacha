use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::{Index, IndexMut};

use crate::big::secure::raw::BaseType;

pub trait StorageType:
    core::fmt::Debug + AsRef<[BaseType]> + Index<usize, Output = BaseType>
{
}

pub trait StorageTypeMut: StorageType + AsMut<[BaseType]> + IndexMut<usize> {
    fn truncate(&mut self, length: usize);
}

impl<const LEN: usize> StorageType for [BaseType; LEN] {}
impl<const LEN: usize> StorageTypeMut for [BaseType; LEN] {
    fn truncate(&mut self, length: usize) {
        //
    }
}

impl StorageType for Vec<BaseType> {}
impl StorageTypeMut for Vec<BaseType> {
    fn truncate(&mut self, length: usize) {
        assert!(length <= self.len());
        self.resize(length, 0);
    }
}

#[derive(Debug)]
pub struct BaseSlice<'a> {
    value: &'a [BaseType],
}

impl<'a> Index<usize> for BaseSlice<'a> {
    type Output = BaseType;

    fn index(&self, index: usize) -> &Self::Output {
        self.value.index(index)
    }
}

impl<'a> AsRef<[BaseType]> for BaseSlice<'a> {
    fn as_ref(&self) -> &[BaseType] {
        &self.value
    }
}

impl<'a> StorageType for BaseSlice<'a> {}

#[derive(Debug)]
pub struct BaseSliceMut<'a> {
    value: &'a mut [BaseType],
    len: usize,
}

impl<'a> Index<usize> for BaseSliceMut<'a> {
    type Output = BaseType;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len);
        self.value.index(index)
    }
}

impl<'a> IndexMut<usize> for BaseSliceMut<'a> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.len);
        self.value.index_mut(index)
    }
}

impl<'a> AsRef<[BaseType]> for BaseSliceMut<'a> {
    fn as_ref(&self) -> &[BaseType] {
        &self.value[0..self.len]
    }
}

impl<'a> AsMut<[BaseType]> for BaseSliceMut<'a> {
    fn as_mut(&mut self) -> &mut [BaseType] {
        &mut self.value[0..self.len]
    }
}

impl<'a> StorageType for BaseSliceMut<'a> {}
impl<'a> StorageTypeMut for BaseSliceMut<'a> {
    fn truncate(&mut self, length: usize) {
        assert!(length <= self.len);
        self.len = length;
    }
}

pub trait Allocator<'a> {
    type Storage: StorageTypeMut;

    type SubAllocator<'b>: Allocator<'b>
    where
        Self: 'b;

    /// Allocates a zero'd number
    fn allocate(&self, len: usize) -> Self::Storage;

    fn sub_allocator<'b>(&'b mut self) -> Self::SubAllocator<'b>;
}

pub struct HeapAllocator {}

impl<'a> Allocator<'a> for HeapAllocator {
    type Storage = Vec<BaseType>;

    type SubAllocator<'b> = HeapAllocator;

    fn allocate(&self, len: usize) -> Self::Storage {
        vec![0; len]
    }

    fn sub_allocator<'b>(&'b mut self) -> Self::SubAllocator<'b> {
        HeapAllocator {}
    }
}

pub struct Arena {
    state: RefCell<ArenaState>,
}

impl Arena {
    pub fn new(size: usize) -> Self {
        Self {
            state: RefCell::new(ArenaState {
                data: vec![0; size],
                offset: 0,
                peak_offset: 0,
            }),
        }
    }

    /// MUST BE 'mut' to ensure that scoped allocators work as expected.
    pub fn allocator<'a>(&'a mut self) -> ArenaAllocator<'a> {
        ArenaAllocator {
            arena: self,
            start_offset: 0,
        }
    }

    pub fn peak_allocated(&self) -> usize {
        self.state.borrow().peak_offset
    }
}

struct ArenaState {
    data: Vec<BaseType>,
    offset: usize,
    peak_offset: usize,
}

pub struct ArenaAllocator<'a> {
    arena: &'a Arena,
    start_offset: usize,
}

impl<'a> Allocator<'a> for ArenaAllocator<'a> {
    type Storage = BaseSliceMut<'a>;
    type SubAllocator<'b> = ArenaAllocator<'b> where Self: 'b;

    fn allocate(&self, len: usize) -> Self::Storage {
        self.allocate_impl(len)
    }

    fn sub_allocator<'b>(&'b mut self) -> Self::SubAllocator<'b> {
        self.sub_allocator_impl()
    }
}

impl<'a> ArenaAllocator<'a> {
    fn allocate_impl(&self, len: usize) -> BaseSliceMut<'a> {
        let mut state = self.arena.state.borrow_mut();
        let i = state.offset;
        state.offset += len;

        if state.offset > state.peak_offset {
            state.peak_offset = state.offset;
        }

        let value = &mut state.data[i..(i + len)];

        // Changing the lifetime.
        let value = unsafe {
            let len = value.len();

            let ptr = value.as_mut_ptr();

            core::slice::from_raw_parts_mut::<'a>(ptr, len)
        };

        for i in value.iter_mut() {
            *i = 0;
        }

        BaseSliceMut { value, len }
    }

    fn sub_allocator_impl<'b>(&'b mut self) -> ArenaAllocator<'b> {
        let start_offset = self.arena.state.borrow_mut().offset;

        ArenaAllocator {
            arena: self.arena,
            start_offset,
        }
    }
}

impl<'a> Drop for ArenaAllocator<'a> {
    fn drop(&mut self) {
        self.arena.state.borrow_mut().offset = self.start_offset;
    }
}
