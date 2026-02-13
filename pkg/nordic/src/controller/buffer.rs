use core::marker::PhantomData;

use peripherals_proto::peripherals::PeripheralResponse;

use crate::controller::allocator::*;


pub struct Buffer {
    buffer: BoxedSlice<u8>,
    used: usize,
    consumed: usize,
}

impl Buffer {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: BoxedSlice::new_zeroed(size),
            used: 0,
            consumed: 0,
        }
    }

    pub fn read(&mut self, res: &mut PeripheralResponse) {
        let data: &[u8] = &self.buffer[..self.used];

        let offset = self.consumed;
        if offset >= data.len() {
            return;
        }

        let offset_end = core::cmp::min(offset + res.data_val_mut().capacity(), data.len());
        self.consumed = offset_end;

        res.data_val_mut().extend_from_slice(&data[offset..offset_end]);
    }

    pub fn view_mut<'a, T: Primitive>(&'a mut self) -> BufferViewMut<'a, T> {
        BufferViewMut {
            buffer: self,
            t: PhantomData
        }
    }
}

pub struct BufferViewMut<'a, T: Primitive> {
    buffer: &'a mut Buffer,
    t: PhantomData<T>
}

impl<'a, T: Primitive> BufferViewMut<'a, T> {
    pub fn raw(&mut self) -> &mut [T] {
        let len = self.buffer.buffer.len() / core::mem::size_of::<T>();

        unsafe {
            core::slice::from_raw_parts_mut(
                self.buffer.buffer.as_ptr() as *mut T,
                len
            )
        }
    }

    pub fn used(&self) -> usize {
        self.buffer.used
    }

    pub fn set_used(&mut self, n: usize) {
        self.buffer.used = n * core::mem::size_of::<T>();
        self.buffer.consumed = 0;
    }
}