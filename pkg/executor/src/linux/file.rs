use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::ffi::CString;
use std::sync::Arc;

use common::errors::*;
use common::io::IoError;
use sys::{
    c_int, close, open, read, Errno, IoSlice, IoSliceMut, IoUringOp, OpenFileDescriptor, RWFlags,
    O_CLOEXEC, O_NONBLOCK, O_RDONLY,
};

use crate::linux::executor::FileDescriptor;
use crate::linux::io_uring::ExecutorOperation;
use crate::RemapErrno;

#[derive(Clone, Copy, Debug)]
pub struct SyncRange {
    pub start: u64,
    pub end: u64,
}

/// Generic wrapper around a Linux file descriptor for performing common I/O
/// operations.
///
/// DO NOT USE DIRECTLY: Prefer to use the file wrappers such as those in the
/// 'net and 'file' crates.
///
/// NOTE: It is generally not a good idea to directly use this as it doesn't
/// account for file type specific requirements (like seekability).
#[derive(Clone)]
pub struct FileHandle {
    fd: Arc<OpenFileDescriptor>,

    /// If the file descriptor is seekable, the position at which we will next
    /// read/write for operations not specifying an explicit offset.
    offset: Option<u64>,
}

impl FileHandle {
    /// NOTE: If the file is seekable, we assume it is currently at offset 0.
    ///
    /// TODO: Make this unsafe to discourage usage?
    pub fn new(fd: OpenFileDescriptor, seekable: bool) -> Self {
        Self {
            fd: Arc::new(fd),
            offset: if seekable { Some(0) } else { None },
        }
    }

    pub unsafe fn as_raw_fd(&self) -> &OpenFileDescriptor {
        &self.fd
    }

    pub async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        // TODO: Only up to 2^32 bytes can be read in one operation right?
        let buffers = [IoSliceMut::new(output)];
        self.read_vectored(&buffers).await
    }

    pub async fn read_vectored(&mut self, output: &[IoSliceMut<'_>]) -> Result<usize> {
        let mut zero = 0;
        let mut offset = self.offset.as_mut().unwrap_or(&mut zero);

        let op = ExecutorOperation::submit(IoUringOp::ReadV {
            fd: **self.fd,
            offset: *offset,
            buffers: output,
            flags: RWFlags::empty(),
        })
        .await?;

        let res = op.wait().await?;
        let n = res.readv_result().remap_errno::<IoError>()?;

        *offset += n as u64;

        Ok(n)
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<usize> {
        let mut zero = 0;
        let mut offset = self.offset.as_mut().unwrap_or(&mut zero);

        let n = Self::write_impl(&self.fd, *offset, data).await?;
        *offset += n as u64;

        Ok(n)
    }

    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<usize> {
        Self::write_impl(&self.fd, offset, data).await
    }

    async fn write_impl(fd: &Arc<OpenFileDescriptor>, offset: u64, data: &[u8]) -> Result<usize> {
        let buffers = [IoSlice::new(data)];

        let op = ExecutorOperation::submit(IoUringOp::WriteV {
            fd: ***fd,
            offset,
            buffers: &buffers,
            flags: RWFlags::empty(),
        })
        .await?;

        let res = op.wait().await?;
        let n = res.writev_result().remap_errno::<IoError>()?;
        Ok(n)
    }

    /// NOTE: Raw Errno failures will be returned for fsync errors.
    pub async fn sync(&self, data_sync: bool, range: Option<SyncRange>) -> Result<()> {
        let mut offset = None;
        let mut length = None;
        if let Some(range) = range {
            offset = Some(range.start);

            if range.start > range.end || range.end - range.start > (core::u32::MAX as u64) {
                return Err(err_msg("Invalid or too large of a sync range"));
            }

            if range.start == range.end {
                return Ok(());
            }

            length = Some((range.end - range.start) as u32);
        }

        let op = ExecutorOperation::submit(IoUringOp::Fsync {
            fd: **self.fd,
            data_sync,
            offset,
            length,
        })
        .await?;

        let res = op.wait().await?;

        Ok(res.fsync_result()?)
    }

    pub fn seek(&mut self, offset: u64) {
        assert!(self.offset.is_some());
        self.offset = Some(offset);
    }

    pub fn current_position(&self) -> u64 {
        self.offset.unwrap()
    }
}

/*
If I have a reference to a field, then that field trivially can't be moved until I drop my reference.
*/

/*
struct ReadFuture<'a> {
    polling_context: PollingContext,

    file: &'a LocalFile,

    output: &'a mut [u8],

    nread: usize,

    /// If true, we will continue reading until the buffer is full is we reached
    /// the end of the file.
    exact: bool,
}

fn advance_mut(data: &mut &mut [u8], n: usize) {
    *data = &mut core::mem::take(data)[n..];
}

impl<'a> Future for ReadFuture<'a> {
    type Output = Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<usize>> {
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            if this.output.len() == 0 {
                break;
            }

            let mut buf = [0u8; 4];

            let n = match unsafe { read(this.file.fd, buf.as_mut_ptr(), this.output.len()) } {
                Ok(n) => n,
                Err(Errno::EAGAIN) => {
                    return Poll::Pending;
                }
                Err(e) => {
                    return Poll::Ready(Err(e.into()));
                }
            };

            this.nread += n;

            advance_mut(&mut this.output, n);

            if n == 0 || !this.exact {
                break;
            }
        }

        Poll::Ready(Ok(this.nread))
    }
}
 */
