use core::ops::{Deref, DerefMut};
use std::fmt::Debug;
use std::pin::Pin;

use cxx::UniquePtr;

use crate::control::Control;
use crate::control_id::ControlId;
use crate::control_value::{AssignToControlValue, ControlValue, FromControlValue};
use crate::ffi;

pub struct ControlListOwned {
    ptr: UniquePtr<ffi::ControlList>,
}

impl Deref for ControlListOwned {
    type Target = ControlList;

    fn deref(&self) -> &Self::Target {
        unsafe {
            core::mem::transmute::<&ffi::ControlList, &ControlList>(self.ptr.as_ref().unwrap())
        }
    }
}

impl DerefMut for ControlListOwned {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            core::mem::transmute::<Pin<&mut ffi::ControlList>, &mut ControlList>(
                self.ptr.as_mut().unwrap(),
            )
        }
    }
}

#[repr(transparent)]
pub struct ControlList {
    raw: ffi::ControlList,
}

impl ControlList {
    // TODO: This is specifically a control list for controls and not properties.
    pub fn new() -> ControlListOwned {
        ControlListOwned {
            ptr: unsafe { ffi::new_control_list() },
        }
    }

    /// May return None if the control is missing from the list or there is a
    /// type mismatch.
    pub fn get<'a, T: FromControlValue<'a>>(&'a self, control: Control<T>) -> Option<T::Target> {
        self.get_by_num::<T>(control.id())
    }

    pub fn get_by_id<'a, T: FromControlValue<'a>>(
        &'a self,
        control_id: &ControlId,
    ) -> Option<T::Target> {
        self.get_by_num::<T>(control_id.id())
    }

    pub fn get_by_num<'a, T: FromControlValue<'a>>(
        &'a self,
        control_num: u32,
    ) -> Option<T::Target> {
        if !self.raw.contains(control_num) {
            return None;
        }

        T::from_value(self.raw.get(control_num))
    }

    pub fn set<T: AssignToControlValue, V: Into<T>>(&mut self, control: Control<T>, value: V) {
        let mut raw_value = ffi::new_control_value();
        value.into().assign_to_value(raw_value.as_mut().unwrap());

        let p = unsafe { Pin::new_unchecked(&mut self.raw) };
        p.set(control.id(), &raw_value);
    }

    /*
    pub fn set(&mut self, id: &ffi::ControlId, value: &ControlValue) {
        // TODO: Check for a type match?

        // NOTE: We assume that libcamera will behave ok if the control is not defined
        // for the camera.

        let mut native_value = ffi::new_control_value();
        value.assign_to(native_value.as_mut().unwrap());

        let p = unsafe { Pin::new_unchecked(&mut self.raw) };
        p.set(id.id(), &native_value);
    }
    */
}

impl<'a> From<&'a ffi::ControlList> for &'a ControlList {
    fn from(value: &'a ffi::ControlList) -> Self {
        unsafe { core::mem::transmute(value) }
    }
}

impl<'a> From<&'a mut ffi::ControlList> for &'a mut ControlList {
    fn from(value: &'a mut ffi::ControlList) -> Self {
        unsafe { core::mem::transmute(value) }
    }
}

impl Debug for ControlList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entries = ffi::control_list_entries(&self.raw);

        // TODO: Debup this logic.
        let id_map = {
            let p = self.raw.idMap();
            assert!(p != core::ptr::null());
            unsafe { &*p }
        };

        let mut s = f.debug_struct("ControlList");

        for entry in entries {
            let field_name = if id_map.contains(&entry.key) {
                unsafe { &**id_map.at(&entry.key) }.name().to_string()
            } else {
                format!("Unknown({})", entry.key)
            };

            s.field(&field_name, &ffi::control_value_to_string(entry.value));
        }

        s.finish()
    }
}
