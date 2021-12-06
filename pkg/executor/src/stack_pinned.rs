// See https://github.com/rust-lang/rfcs/blob/master/text/2349-pin.md#stack-pinning-api-potential-future-extension

use core::marker::PhantomData;
use core::pin::Pin;

pub fn stack_pinned<'a, T>(data: T) -> StackPin<'a, T> {
    StackPin {
        data,
        _marker: PhantomData,
    }
}

pub struct StackPin<'a, T: 'a> {
    data: T,
    _marker: PhantomData<&'a &'a mut ()>,
}

impl<'a, T> StackPin<'a, T> {
    pub fn into_pin(&'a mut self) -> Pin<&'a mut T> {
        unsafe { Pin::new_unchecked(&mut self.data) }
    }
}
