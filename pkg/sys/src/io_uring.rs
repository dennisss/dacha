use core::mem::transmute;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use peripherals_raw::register::RawRegister;

use crate::file::OpenFileDescriptor;
use crate::iov::{IoSlice, IoSliceMut, RWFlags};
use crate::mapped_memory::MappedMemory;
use crate::socket::{SocketAddressAndLength, SocketFlags};
use crate::{bindings, c_int, c_size_t, c_uint, c_void, kernel, Errno};

/*
TODO: MAybe use IORING_SETUP_SINGLE_ISSUER

Some challenges with urings:
- To allow for future cancellations, we do need to block any thread which has submitted a op with allocated memory.
    - Can be avoided if we have the io_uring executer own the memory.
    - Doesn't need to happen for things like timers.
    - Need to leave some submit queue entries reserved for cancellations.
- Avoid starvations:
    - e.g. if a request is i/o heavy, it may prevent new accept ops for happening.
    - Generally limit the number of submissions per task.


What we'd use as data:
- Index of an active request
- poll operation
    - Initially submit an op and acquire a request id
    - when polled again, check in the slab to see if it was complete yet.
    - The slab entry is only ever set once so this could use an atomic


*/

/// Number of
const NUM_ENTRIES_SUGGESTION: u32 = 128;

/// Type used for fields in the ring structures (head, tail indices, etc.).
type RingIndex = u32;

pub struct IoUring {
    fd: OpenFileDescriptor,
    submission_queue: SubmissionQueue,
    completion_queue: CompletionQueue,
}

impl IoUring {
    pub fn create() -> Result<Self> {
        let mut params = bindings::io_uring_params::default();

        let fd = OpenFileDescriptor::new(unsafe {
            raw::io_uring_setup(NUM_ENTRIES_SUGGESTION, &mut params)
        }?);

        let submit_queue_ring = unsafe {
            MappedMemory::create(
                core::ptr::null_mut(),
                (params.sq_off.array as usize)
                    + (params.sq_entries as usize) * core::mem::size_of::<u32>(),
                bindings::PROT_READ | bindings::PROT_WRITE,
                bindings::MAP_SHARED | bindings::MAP_POPULATE,
                *fd,
                bindings::IORING_OFF_SQ_RING as usize,
            )
        }?;

        /*
        Each value is a io_uring_sqe
        */
        let submit_queue_entries = unsafe {
            MappedMemory::create(
                core::ptr::null_mut(),
                (params.sq_entries as usize) * core::mem::size_of::<bindings::io_uring_sqe>(),
                bindings::PROT_READ | bindings::PROT_WRITE,
                bindings::MAP_SHARED | bindings::MAP_POPULATE,
                *fd,
                bindings::IORING_OFF_SQES as usize,
            )
        }?;

        let completion_queue_ring = unsafe {
            MappedMemory::create(
                core::ptr::null_mut(),
                (params.cq_off.cqes as usize)
                    + (params.cq_entries as usize) * core::mem::size_of::<bindings::io_uring_cqe>(),
                bindings::PROT_READ | bindings::PROT_WRITE,
                bindings::MAP_SHARED | bindings::MAP_POPULATE,
                *fd,
                bindings::IORING_OFF_CQ_RING as usize,
            )
        }?;

        let mut submission_queue = SubmissionQueue {
            offsets: params.sq_off,
            ring: submit_queue_ring,
            entries: submit_queue_entries,
        };

        let mut completion_queue = CompletionQueue {
            offsets: params.cq_off,
            ring: completion_queue_ring,
        };

        if submission_queue.num_entries() != params.sq_entries
            || completion_queue.num_entries() != params.cq_entries
        {
            todo!()
        }

        Ok(Self {
            fd,
            submission_queue,
            completion_queue,
        })
    }

    pub fn split(self) -> (IoSubmissionUring, IoCompletionUring) {
        let fd = Arc::new(self.fd);

        (
            IoSubmissionUring {
                fd: fd.clone(),
                inner: self.submission_queue,
            },
            IoCompletionUring {
                fd: fd.clone(),
                inner: self.completion_queue,
            },
        )
    }
}

pub struct IoSubmissionUring {
    fd: Arc<OpenFileDescriptor>,
    inner: SubmissionQueue,
}

impl IoSubmissionUring {
    /// Submits an operation to be asyncronously processed.
    ///
    /// Note that this only returns once the kernel has consumed the
    /// request, but any references in 'op' must stay alive until the operation
    /// has been marked as completed.
    pub unsafe fn submit(&mut self, op: IoUringOp, user_data: u64) -> Result<()> {
        self.submit_impl(op, user_data)
    }

    /// Non-unsafe impl of submit() to ensure we limit the amount of unsafe code
    /// we use.
    fn submit_impl(&mut self, op: IoUringOp, user_data: u64) -> Result<()> {
        let mut entry = bindings::io_uring_sqe::default();

        let mut timespec = None;

        match op {
            IoUringOp::Invalid => {
                entry.opcode = 0xFF;
            }

            IoUringOp::Noop => {
                entry.opcode = bindings::IORING_OP_NOP as u8;
            }
            IoUringOp::ReadV {
                fd,
                offset,
                buffers,
                flags,
            } => {
                entry.opcode = bindings::IORING_OP_READV as u8;
                entry.fd = fd;
                entry.off = offset;
                entry.addr = unsafe { core::mem::transmute(buffers.as_ptr()) };
                entry.len = buffers.len() as u32;
                entry.__bindgen_anon_1.rw_flags = flags.to_raw();
            }
            IoUringOp::WriteV {
                fd,
                offset,
                buffers,
                flags,
            } => {
                entry.opcode = bindings::IORING_OP_WRITEV as u8;
                entry.fd = fd;
                entry.off = offset;
                entry.addr = unsafe { core::mem::transmute(buffers.as_ptr()) };
                entry.len = buffers.len() as u32;
                entry.__bindgen_anon_1.rw_flags = flags.to_raw();
            }
            IoUringOp::Accept {
                fd,
                sockaddr,
                flags,
            } => {
                entry.opcode = kernel::io_uring_op::IORING_OP_ACCEPT as u8;
                entry.fd = fd;

                sockaddr.reset();
                entry.addr = unsafe { core::mem::transmute(&sockaddr.addr) };
                entry.off = unsafe { core::mem::transmute(&sockaddr.len) }; // addr2 field.
                entry.__bindgen_anon_1.fsync_flags = flags.to_raw() as u32; // accept_flags field.
            }
            IoUringOp::Timeout { duration } => {
                entry.opcode = kernel::io_uring_op::IORING_OP_TIMEOUT as u8;

                // NOTE: The kernel copies the this data during op submission so we don't need
                // to worry about keeping this alive from longer than this function.
                let t = timespec.insert(kernel::timespec64::from(duration));
                entry.addr = unsafe { core::mem::transmute(t) };

                entry.len = 1; // Only 1 timespec is provided.

                entry.__bindgen_anon_1.fsync_flags = 0; // timeout_flags field. Use a relative timeout.
                entry.off = 0; // Only count timeouts. Ignore other completions.
            }
        }

        entry.user_data = user_data;
        entry.flags = 0; // TODO
        entry.ioprio = 0; // TODO: Only supported for some.

        assert!(self.inner.push(entry));

        // TODO: Check what IORING_ENTER_REGISTERED_RING does.

        /*
        For waiting use IORING_ENTER_GETEVENTS?
        */

        unsafe { io_uring_enter(&self.fd, 1, 0, IoUringEnterFlags::empty(), 0, None) }?;

        /*
        // TODO: Check this whenever we use enteR?

        let n = unsafe { *self.submission_queue.dropped() };
        println!("Dropped: {}", n);
        */

        Ok(())
    }
}

struct SubmissionQueue {
    offsets: bindings::io_sqring_offsets,

    ring: MappedMemory,

    /// Memory buffer containing all the ring entries (io_uring_sqe structs).
    entries: MappedMemory,
}

impl SubmissionQueue {
    /// Returns whether or not the entry could be added (will return false if
    /// the queue is full).
    pub fn push(&mut self, entry: bindings::io_uring_sqe) -> bool {
        // TODO: Use atomic reads?
        let head = self.head().read();
        let mut tail = self.tail().read();

        // Check if the ring is full.
        if head.wrapping_add(self.num_entries()) == tail {
            return false;
        }

        // NOTE: The ring_mask should never change.
        let index = tail & self.ring_mask();

        unsafe {
            core::ptr::write_volatile(self.entries().add(index as usize), entry);
            core::ptr::write_volatile(self.array().add(index as usize), index);
        }

        tail = tail.wrapping_add(1);

        self.tail().write(tail);

        true
    }

    fn head(&self) -> &RawRegister<RingIndex> {
        unsafe { transmute(self.ring.addr().add(self.offsets.head as usize)) }
    }

    fn tail(&mut self) -> &mut RawRegister<RingIndex> {
        unsafe { transmute(self.ring.addr().add(self.offsets.tail as usize)) }
    }

    /// TODO: Check that this is consistent with the params.
    /// NOTE: This should be immutable after the creation of the ring.
    fn ring_mask(&self) -> RingIndex {
        unsafe { *(self.ring.addr().add(self.offsets.ring_mask as usize) as *const RingIndex) }
    }

    /// TODO: Check that this is consistent with the params.
    /// NOTE: This should be immutable after the creation of the ring.
    fn num_entries(&self) -> RingIndex {
        unsafe { *(self.ring.addr().add(self.offsets.ring_entries as usize) as *const RingIndex) }
    }

    /// TODO: Check this.
    fn flags(&mut self) -> *mut RingIndex {
        unsafe { self.ring.addr().add(self.offsets.flags as usize) as *mut RingIndex }
    }

    /// TODO: Check this.
    fn dropped(&mut self) -> *const RingIndex {
        unsafe { self.ring.addr().add(self.offsets.dropped as usize) as *mut RingIndex }
    }

    fn array(&mut self) -> *mut RingIndex {
        unsafe { self.ring.addr().add(self.offsets.array as usize) as *mut RingIndex }
    }

    fn entries(&mut self) -> *mut bindings::io_uring_sqe {
        self.entries.addr() as *mut bindings::io_uring_sqe
    }
}

pub struct IoCompletionUring {
    fd: Arc<OpenFileDescriptor>,
    inner: CompletionQueue,
}

impl IoCompletionUring {
    /// Blocks until at least one op has completed and can be retrieved with
    /// retrieve() or the timeout has elapsed. It's possible that it may return
    /// earlier if a signal interrupts it.
    pub fn wait(&mut self, timeout: Option<Duration>) -> Result<(), Errno> {
        // TODO: Indicate to the user if a EINTR error occurs (signal interrupted us
        // while waiting).

        let res = unsafe {
            io_uring_enter(
                &self.fd,
                0,
                1,
                IoUringEnterFlags::IORING_ENTER_GETEVENTS,
                0,
                timeout,
            )
        };

        if let Err(e) = res {
            // EINTR: Interrupted by a signal
            // ETIME: Timeout elapsed.
            if e == Errno::EINTR || e == Errno::ETIME {
                return Ok(());
            }

            return Err(e);
        }

        Ok(())
    }

    /// Tries to retrieve a single completion entry (or returns None if none is
    /// available).
    pub fn retrieve(&mut self) -> Option<IoUringCompletion> {
        // TODO: Check if any overflowed.

        let mut entry = bindings::io_uring_cqe::default();
        let n = self.inner.pop(core::slice::from_mut(&mut entry));
        if n == 1 {
            Some(IoUringCompletion {
                user_data: entry.user_data,
                result: IoUringResult {
                    code: entry.res,
                    flags: entry.flags,
                },
            })
        } else {
            None
        }
    }

    /// Returns the maximum number of completion entries this ring can store at
    /// a given point in time.
    pub fn capacity(&self) -> usize {
        self.inner.num_entries() as usize
    }
}

struct CompletionQueue {
    offsets: bindings::io_cqring_offsets,
    ring: MappedMemory,
}

impl CompletionQueue {
    pub fn pop(&mut self, out: &mut [bindings::io_uring_cqe]) -> usize {
        let mut head = self.head().read();
        let tail = self.tail().read();

        let mut n = 0;

        while head != tail && n < out.len() {
            let index = head & self.ring_mask();

            out[n] = unsafe { core::ptr::read_volatile(self.entries().add(index as usize)) };

            head = head.wrapping_add(1);
            n += 1;
        }

        self.head().write(head);

        n
    }

    fn head(&self) -> &mut RawRegister<RingIndex> {
        unsafe { transmute(self.ring.addr().add(self.offsets.head as usize)) }
    }

    fn tail(&mut self) -> &RawRegister<RingIndex> {
        unsafe { transmute(self.ring.addr().add(self.offsets.tail as usize)) }
    }

    /// TODO: Check that this is consistent with the params.
    /// NOTE: This should be immutable after the creation of the ring.
    fn ring_mask(&self) -> RingIndex {
        unsafe { *(self.ring.addr().add(self.offsets.ring_mask as usize) as *const RingIndex) }
    }

    /// TODO: Check that this is consistent with the params.
    /// NOTE: This should be immutable after the creation of the ring.
    fn num_entries(&self) -> RingIndex {
        unsafe { *(self.ring.addr().add(self.offsets.ring_entries as usize) as *const RingIndex) }
    }

    /// TODO: Check this.
    fn overflow(&mut self) -> *const RingIndex {
        unsafe { self.ring.addr().add(self.offsets.overflow as usize) as *mut RingIndex }
    }

    fn entries(&mut self) -> *mut bindings::io_uring_cqe {
        unsafe { self.ring.addr().add(self.offsets.cqes as usize) as *mut bindings::io_uring_cqe }
    }
}

// NOTE: If a file is not seekable, the offset should be 0 or -1

pub enum IoUringOp<'a> {
    Noop,
    ReadV {
        fd: c_int,
        offset: u64,
        buffers: &'a [IoSliceMut<'a>],
        flags: RWFlags,
    },
    WriteV {
        fd: c_int,
        offset: u64,
        buffers: &'a [IoSlice<'a>],
        flags: RWFlags,
    },

    Accept {
        fd: c_int,
        sockaddr: &'a mut SocketAddressAndLength,
        flags: SocketFlags,
    },

    /// Waits until at least the given duration has elapsed on the Linux
    /// CLOCK_MONOTONIC.
    Timeout {
        duration: Duration,
    },

    // FSync,
    /// An invalid operation that the kernel will drop.
    Invalid,
}

impl<'a> IoUringOp<'a> {
    pub fn try_into_static(&self) -> Option<IoUringOp<'static>> {
        match self {
            IoUringOp::Noop => Some(IoUringOp::Noop),
            IoUringOp::Timeout { duration } => Some(IoUringOp::Timeout {
                duration: duration.clone(),
            }),
            IoUringOp::ReadV { .. } => None,
            IoUringOp::WriteV { .. } => None,
            IoUringOp::Accept { .. } => None,
            IoUringOp::Invalid => Some(IoUringOp::Invalid),
        }
    }
}

pub struct IoUringCompletion {
    pub user_data: u64,
    pub result: IoUringResult,
}

pub struct IoUringResult {
    code: i32,
    flags: u32,
}

impl IoUringResult {
    /// Interprets the result as the result of a ReadV op.
    ///
    /// Returns the total number of bytes read.
    pub fn readv_result(&self) -> Result<usize, Errno> {
        if self.code < 0 {
            return Err(Errno(-self.code as i64));
        }

        Ok(self.code as usize)
    }

    pub fn writev_result(&self) -> Result<usize, Errno> {
        self.readv_result()
    }

    pub fn accept_result(&self) -> Result<OpenFileDescriptor, Errno> {
        if self.code < 0 {
            return Err(Errno(-self.code as i64));
        }

        Ok(OpenFileDescriptor::new(self.code as i32))
    }

    pub fn timeout_result(&self) -> Result<(), Errno> {
        if self.code != 0 {
            let e = Errno(-self.code as i64);

            if e == Errno::ETIME {
                return Ok(());
            }

            return Err(e);
        }

        Ok(())
    }
}

unsafe fn io_uring_enter(
    fd: &OpenFileDescriptor,
    to_submit: usize,
    min_complete: usize,
    flags: IoUringEnterFlags,
    signal_mask: kernel::sigset_t,
    timeout: Option<Duration>,
) -> Result<usize, Errno> {
    let timespec = timeout.map(|v| kernel::timespec::from(v));

    let arg = kernel::io_uring_getevents_arg {
        sigmask: core::mem::transmute(&signal_mask),
        sigmask_sz: core::mem::size_of::<kernel::sigset_t>() as u32,
        pad: 0,
        ts: timeout
            .as_ref()
            .map(|v| core::mem::transmute(v))
            .unwrap_or(0),
    };

    Ok(raw::io_uring_enter(
        **fd,
        to_submit as u32,
        min_complete as u32,
        (flags | IoUringEnterFlags::IORING_ENTER_EXT_ARG).to_raw(),
        core::mem::transmute(&arg),
        core::mem::size_of::<kernel::io_uring_getevents_arg>(),
    )? as usize)
}

define_bit_flags!(IoUringEnterFlags u32 {
    IORING_ENTER_GETEVENTS = (bindings::IORING_ENTER_GETEVENTS),
    IORING_ENTER_EXT_ARG = (kernel::IORING_ENTER_EXT_ARG)
});

mod raw {
    use super::*;

    syscall!(
        io_uring_setup,
        bindings::SYS_io_uring_setup,
        entries: u32,
        params: *mut bindings::io_uring_params => Result<c_int>
    );

    syscall!(
        io_uring_enter,
        bindings::SYS_io_uring_enter,
        fd: c_int,
        to_submit: u32,
        min_complete: u32,
        flags: u32,
        argp: *const c_void,
        argsz: c_size_t => Result<c_int>
    );

    syscall!(
        io_uring_register,
        bindings::SYS_io_uring_register,
        fd: c_int,
        opcode: c_int,
        arg: *mut c_void,
        nr_args: c_int => Result<()>
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_sizes() {
        // If these become bigger, then we need to use IORING_SETUP_SQE128 or
        // IORING_SETUP_CQE32.
        assert_eq!(core::mem::size_of::<bindings::io_uring_sqe>(), 64);
        assert_eq!(core::mem::size_of::<bindings::io_uring_cqe>(), 16);
    }
}
