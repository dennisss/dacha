use std::marker::PhantomData;

use crate::{bindings, c_int, c_void};

#[repr(transparent)]
pub struct IoSlice<'a> {
    raw: bindings::iovec,
    lifetime: PhantomData<&'a ()>,
}

unsafe impl Send for IoSlice<'_> {}
unsafe impl Sync for IoSlice<'_> {}

impl<'a> IoSlice<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut raw = bindings::iovec::default();
        raw.iov_base = data.as_ptr() as *mut c_void;
        raw.iov_len = data.len();

        Self {
            raw,
            lifetime: PhantomData,
        }
    }
}

/// NOTE: For this to be safe to use, this MUST NOT be cloneable.
#[repr(transparent)]
pub struct IoSliceMut<'a> {
    raw: bindings::iovec,
    lifetime: PhantomData<&'a mut ()>,
}

unsafe impl Send for IoSliceMut<'_> {}
unsafe impl Sync for IoSliceMut<'_> {}

impl<'a> IoSliceMut<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        let mut raw = bindings::iovec::default();
        raw.iov_base = data.as_ptr() as *mut c_void;
        raw.iov_len = data.len();

        Self {
            raw,
            lifetime: PhantomData,
        }
    }
}

define_bit_flags!(RWFlags c_int {
    RWF_HIPRI = 0x00000001,
    RWF_DSYNC = 0x00000002,
    RWF_SYNC = 0x00000004,
    RWF_NOWAIT = 0x00000008,
    RWF_APPEND = 0x00000010
});
