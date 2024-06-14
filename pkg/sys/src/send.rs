use std::marker::PhantomData;

use common::aligned::AlignedVec;

use crate::{
    bindings, c_int, c_size_t, Errno, IoSlice, IoSliceMut, SocketAddr, SocketAddressAndLength,
};

pub fn sendmsg(fd: c_int, msg: &MessageHeader, flags: c_int) -> Result<usize, Errno> {
    unsafe { raw::sendmsg(fd, &msg.inner, flags) }
}

pub fn recvmsg(fd: c_int, msg: &mut MessageHeaderMut, flags: c_int) -> Result<usize, Errno> {
    unsafe { raw::recvmsg(fd, &mut msg.inner, flags) }
}

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
        control_messages: Option<&'a ControlMessageBuffer>,
    ) -> Self {
        let mut inner = bindings::msghdr::default();

        inner.msg_iov = unsafe { core::mem::transmute(data.as_ptr()) };
        inner.msg_iovlen = data.len();

        if let Some(addr) = addr {
            inner.msg_name = unsafe { core::mem::transmute(addr) };
            inner.msg_namelen = addr.len().unwrap() as u32;
        }

        if let Some(control_messages) = control_messages {
            inner.msg_control = unsafe { core::mem::transmute(control_messages.data.as_ptr()) };
            inner.msg_controllen = control_messages.data.len();
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
    control_messages: Option<&'a mut ControlMessageBuffer>,
    lifetime: PhantomData<&'a mut ()>,
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
        mut control_messages: Option<&'a mut ControlMessageBuffer>,
    ) -> Self {
        let mut inner = bindings::msghdr::default();

        inner.msg_iov = unsafe { core::mem::transmute(data.as_ptr()) };
        inner.msg_iovlen = data.len();

        if let Some(addr) = addr {
            inner.msg_name = unsafe { core::mem::transmute(addr) };
            inner.msg_namelen = core::mem::size_of::<bindings::sockaddr>() as u32;
        }

        if let Some(control_messages) = &mut control_messages {
            inner.msg_control = unsafe { core::mem::transmute(control_messages.data.as_ptr()) };
            inner.msg_controllen = control_messages.data.len();
        }

        Self {
            inner,
            lifetime: PhantomData,
            control_messages,
        }
    }

    pub fn control_messages<'b>(&'b self) -> Option<impl Iterator<Item = ControlMessage> + 'b> {
        let buf = match &self.control_messages {
            Some(v) => v,
            None => return None,
        };

        // recvmsg will change the value of this field to be the actual number of bytes
        // written.
        //
        // TODO: Set this in the ControlMessageBuffer whenever MessageHeaderMut is
        // dropped.
        let len = self.inner.msg_controllen;

        Some(ControlMessageIterator {
            remaining: &buf.data[0..len],
        })
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

/*
internally data is:
- Aligned to both size_of(cmsg_header) and size_of(c_size_t)
- The data is a repeated list of:
    - csmg_header (aligned up to use a multiple of size_of(size_t) bytes)
    - data (aligned up to use a multiple of size_of(size_t) bytes)

*/

/// NOTE: When this is dropped, we may leak any file descriptor references
/// inside of it.
pub struct ControlMessageBuffer {
    data: AlignedVec<u8>,
}

impl ControlMessageBuffer {
    const ALIGNMENT: usize = core::mem::size_of::<c_size_t>();

    /// Aligns a length value up to a multiple of the width of c_size_t.
    fn align(mut len: usize) -> usize {
        let r = len % Self::ALIGNMENT;
        if r != 0 {
            len += Self::ALIGNMENT - r;
        }

        len
    }

    pub fn new(messages: &[ControlMessage]) -> Self {
        let header_aligned_size = Self::align(core::mem::size_of::<bindings::cmsghdr>());

        let mut size = 0;
        for msg in messages {
            size += header_aligned_size;
            size += Self::align(msg.raw_data_size());
        }

        let mut data = AlignedVec::new(size, Self::ALIGNMENT);

        let mut i = 0;

        for msg in messages {
            let hdr = unsafe { core::mem::transmute::<_, &mut bindings::cmsghdr>(&mut data[i]) };
            let data_size = msg.raw_data_size();

            hdr.cmsg_len = header_aligned_size + msg.raw_data_size();
            msg.fill_header(hdr);
            i += header_aligned_size;

            let data = &mut data[i..(i + data_size)];
            msg.fill_data(data);
            i += Self::align(data_size);
        }

        Self { data }
    }
}

#[derive(Debug)]
pub enum ControlMessage {
    ScmRights(Vec<c_int>),
    Unknown,
}

impl ControlMessage {
    fn from_header_and_data(hdr: &bindings::cmsghdr, data: &[u8]) -> Self {
        if hdr.cmsg_level == bindings::SOL_SOCKET as i32
            && hdr.cmsg_type == bindings::SCM_RIGHTS as i32
        {
            assert!(data.len() % 4 == 0);

            let mut fds = vec![];
            for i in 0..(data.len() / 4) {
                fds.push(i32::from_ne_bytes(*array_ref![data, 4 * i, 4]));
            }

            return Self::ScmRights(fds);
        }

        Self::Unknown
    }

    fn fill_header(&self, hdr: &mut bindings::cmsghdr) {
        match self {
            ControlMessage::ScmRights(_) => {
                hdr.cmsg_level = bindings::SOL_SOCKET as i32;
                hdr.cmsg_type = bindings::SCM_RIGHTS as i32;
            }
            ControlMessage::Unknown => {}
        }
    }

    /// Un-padded number of bytes needed to store the data portion of this
    /// message
    fn raw_data_size(&self) -> usize {
        match self {
            ControlMessage::ScmRights(fds) => core::mem::size_of::<c_int>() * fds.len(),
            ControlMessage::Unknown => 0,
        }
    }

    fn fill_data(&self, data: &mut [u8]) {
        match self {
            ControlMessage::ScmRights(fds) => {
                for i in 0..fds.len() {
                    *array_mut_ref![data, i * 4, 4] = fds[i].to_ne_bytes();
                }
            }
            ControlMessage::Unknown => {}
        }
    }

    // fn
}

struct ControlMessageIterator<'a> {
    remaining: &'a [u8],
}

impl<'a> Iterator for ControlMessageIterator<'a> {
    type Item = ControlMessage;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        let header_aligned_size =
            ControlMessageBuffer::align(core::mem::size_of::<bindings::cmsghdr>());
        assert!(self.remaining.len() >= header_aligned_size);

        let header: &bindings::cmsghdr = unsafe { core::mem::transmute(self.remaining.as_ptr()) };
        self.remaining = &self.remaining[header_aligned_size..];

        // TODO: Standard libraries, standard libraries would just return None if this
        // assertion is false.
        assert!(header.cmsg_len >= header_aligned_size);
        let data_len = header.cmsg_len - header_aligned_size;
        let data_padded_len = ControlMessageBuffer::align(data_len);

        assert!(self.remaining.len() >= data_padded_len);
        let data = &self.remaining[0..data_len];
        self.remaining = &self.remaining[data_padded_len..];

        Some(ControlMessage::from_header_and_data(header, data))
    }
}

mod raw {
    use super::*;

    syscall!(sendmsg, bindings::SYS_sendmsg, fd: c_int, msg: *const bindings::msghdr, flags: c_int => Result<c_size_t>);

    syscall!(recvmsg, bindings::SYS_recvmsg, fd: c_int, msg: *mut bindings::msghdr, flags: c_int => Result<c_size_t>);
}
