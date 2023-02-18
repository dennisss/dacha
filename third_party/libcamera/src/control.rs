use std::marker::PhantomData;
use std::ops::Deref;

use crate::ffi::ControlId;
use crate::{AssignToControlValue, FromControlValue};

#[derive(Clone, Copy)]
pub struct Control<T: ?Sized> {
    id: fn() -> &'static ControlId,
    t: PhantomData<T>,
}

// The last two trait bounds here are mainly to catch any unimplemented traits
// for the generated code.
impl<T: ?Sized + AssignToControlValue + for<'a> FromControlValue<'a>> Control<T> {
    /// This is unsafe because we assume that T is compatible with id.typ().
    ///
    /// NOTE: This should only be used in auto generated code in the
    /// crate::controls module.
    pub(crate) const unsafe fn new(id: fn() -> &'static ControlId) -> Self {
        Self { id, t: PhantomData }
    }
}

impl<T: ?Sized> Deref for Control<T> {
    type Target = ControlId;

    fn deref(&self) -> &Self::Target {
        (self.id)()
    }
}
