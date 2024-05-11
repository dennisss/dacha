#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::any::Any;

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;

    fn as_mut_any(&mut self) -> &mut dyn Any;

    #[cfg(feature = "alloc")]
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }

    #[cfg(feature = "alloc")]
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
