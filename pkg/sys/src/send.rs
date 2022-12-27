use std::marker::PhantomData;

use crate::{bindings, IoSlice, IoSliceMut, SocketAddr, SocketAddressAndLength};

/// To be used with sendmsg()
#[repr(C)]
pub struct MessageHeader<'a> {
    // TODO: Ensure this stays aligned with the kernel
    inner: bindings::msghdr,
    lifetime: PhantomData<&'a ()>,
}

// Only contains 'const' references.
unsafe impl Send for MessageHeader<'_> {}
unsafe impl Sync for MessageHeader<'_> {}

impl<'a> MessageHeader<'a> {
    pub fn new(
        data: &'a [IoSlice<'a>],
        addr: Option<&'a SocketAddr>,
        ancillary_data: Option<&'a [u8]>,
    ) -> Self {
        let mut inner = bindings::msghdr::default();

        inner.msg_iov = unsafe { core::mem::transmute(data.as_ptr()) };
        inner.msg_iovlen = data.len();

        if let Some(addr) = addr {
            inner.msg_name = unsafe { core::mem::transmute(addr) };
            inner.msg_namelen = addr.len().unwrap() as u32;
        }

        if let Some(ancillary_data) = ancillary_data {
            inner.msg_control = unsafe { core::mem::transmute(ancillary_data.as_ptr()) };
            inner.msg_namelen = ancillary_data.len() as u32;
        }

        Self {
            inner,
            lifetime: PhantomData,
        }
    }
}

/// To be used with recvmsg()
#[repr(C)]
pub struct MessageHeaderMut<'a> {
    // TODO: Ensure this stays aligned with the kernel
    inner: bindings::msghdr,
    lifetime: PhantomData<&'a ()>,
}

// Has no mechanism of duplicating the 'mut' references in this struct in
// retrieving them.
unsafe impl Send for MessageHeaderMut<'_> {}
unsafe impl Sync for MessageHeaderMut<'_> {}

impl<'a> MessageHeaderMut<'a> {
    /// NOTE: We do not allow the user to directly provide a SocketAddr.
    pub fn new(
        data: &'a [IoSliceMut<'a>],
        addr: Option<&'a MessageHeaderSocketAddrBuffer>,
    ) -> Self {
        let mut inner = bindings::msghdr::default();

        inner.msg_iov = unsafe { core::mem::transmute(data.as_ptr()) };
        inner.msg_iovlen = data.len();

        if let Some(addr) = addr {
            inner.msg_name = unsafe { core::mem::transmute(addr) };
            inner.msg_namelen = core::mem::size_of::<bindings::sockaddr>() as u32;
        }

        Self {
            inner,
            lifetime: PhantomData,
        }
    }

    pub fn reset(&mut self) {
        // TODO: For this and SockAddrAndLength, also reset the sa_family to an invalid
        // value to be able to tell if we retrieved the addres too early.

        self.inner.msg_namelen = core::mem::size_of::<bindings::sockaddr>() as u32;
    }

    pub fn addr(&self) -> Option<SocketAddr> {
        if self.inner.msg_name == core::ptr::null_mut() {
            return None;
        }

        let addr_p: &MessageHeaderSocketAddrBuffer =
            unsafe { core::mem::transmute(self.inner.msg_name) };

        let addr_and_len = SocketAddressAndLength {
            addr: addr_p.inner.clone(),
            len: self.inner.msg_namelen,
        };

        addr_and_len.to_addr()
    }
}

/// Buffer for storing a potentially uninitialized SocketAddr.
///
/// We will be able to unwrap this to a SocketAddr once we get back its length
/// in the MessageHeaderMut struct (use MessageHeaderMut::addr())
#[repr(C)]
pub struct MessageHeaderSocketAddrBuffer {
    inner: SocketAddr,
}

impl MessageHeaderSocketAddrBuffer {
    pub fn new() -> Self {
        Self {
            inner: SocketAddr::empty(),
        }
    }
}
