use std::os::unix::process::CommandExt;
use std::sync::Arc;

use base_error::*;
use common::bytes::Bytes;
use common::io::Readable;
use executor::sync::AsyncMutex;
use executor::{channel, FileHandle};
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use file::{project_path, LocalPath};
use sys::bindings::fuse_opcode;
use sys::bindings::{fuse_in_header, fuse_out_header};
use sys::Errno;

const FUSE_DEVICE_PATH: &'static str = "/dev/fuse";

pub trait RequestHandler {}

pub struct Server {
    shared: Arc<Shared>,
    resources: ServiceResourceGroup,
}

impl_resource_passthrough!(Server, resources);

struct Shared {
    file: FileHandle,

    /// TODO: If we can guarantee writes to finish in one syscall, then we could
    /// probably get rid of this.
    writer_lock: AsyncMutex<()>,
}

impl Server {
    pub async fn create(mount_dir: &LocalPath) -> Result<Self> {
        // TODO: Only need to do this stuff if we don't have mount capabilities.

        let (socket_a, socket_b) = unsafe {
            sys::socketpair(
                sys::AddressFamily::AF_UNIX,
                sys::SocketType::SOCK_STREAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::NONE,
            )?
        };

        // TODO: Need to acquire a lock on forking so that no other processes
        let mut cmd = std::process::Command::new(project_path!("bin/fuse_mount"));

        cmd.arg(format!("--dir={}", mount_dir.as_str()))
            .arg(format!("--socket_fd={}", *socket_b));

        // Remove the CLO_EXEC after the fork() but before the exec().
        unsafe {
            let socket_b_fd = *socket_b;
            cmd.pre_exec(move || {
                sys::fcntl(socket_b_fd, sys::bindings::F_SETFD, 0).unwrap();
                Ok(())
            });
        }

        // TODO: Make this non-blocking
        let code = cmd.spawn()?.wait()?;
        if !code.success() {
            return Err(format_err!("fuse_mount failed with code: {}", code));
        }

        let mut received_messages = vec![];
        {
            let mut buf = [0u8; 16];
            let data = [sys::IoSliceMut::new(&mut buf[..])];

            let mut messages =
                sys::ControlMessageBuffer::new(&[sys::ControlMessage::ScmRights(vec![0])]);

            let mut msg = sys::MessageHeaderMut::new(&data[..], None, Some(&mut messages));

            let n = sys::recvmsg(
                *socket_a,
                &mut msg,
                sys::bindings::MSG_CMSG_CLOEXEC as u32 as i32,
            )?;

            if n != 4 {
                return Err(err_msg("Wrong number of bytes received"));
            }

            for control_msg in msg.control_messages().unwrap() {
                println!("Recv Message: {:?}", control_msg);
                received_messages.push(control_msg);
            }

            if &buf[0..4] != b"FUSE" {
                return Err(err_msg("Didn't get the expected fd payload"));
            }
        }

        // TODO: Make non-blocking
        let fuse_fd = sys::OpenFileDescriptor::new(match received_messages.pop() {
            Some(sys::ControlMessage::ScmRights(fds)) => {
                if fds.len() != 1 {
                    return Err(err_msg("Didn't receive exactly one FUSE fd"));
                }

                fds[0]
            }
            _ => panic!(),
        });

        println!("Got FUSE FD, {}", *fuse_fd);

        let shared = Arc::new(Shared {
            file: FileHandle::new(fuse_fd, false),
            writer_lock: AsyncMutex::new(()),
        });

        let resources = ServiceResourceGroup::new("fuse::Server");
        resources
            .spawn_interruptable("Reader", Self::reader_thread(shared.clone()))
            .await;

        Ok(Self { shared, resources })
    }

    async fn reader_thread(shared: Arc<Shared>) -> Result<()> {
        let mut buffer = vec![0u8; 16 * 1024];
        let mut buffer_len = 0;
        loop {
            let n = shared.file.read_at(0, &mut buffer[buffer_len..]).await?;
            buffer_len += n;

            if n == 0 {
                return Err(err_msg("Read zero bytes"));
            }

            let mut i = 0;
            while i < buffer_len {
                let mut header = sys::bindings::fuse_in_header::default();
                let header_size = match parse_cstruct(&buffer[i..], &mut header) {
                    Some(n) => n,
                    None => break,
                };

                if i + (header.len as usize) > buffer_len {
                    break;
                }

                if header_size > header.len as usize {
                    return Err(err_msg("Entire request smaller than header"));
                }

                println!("Header : {:#?}", header);

                let data = &buffer[(i + header_size)..(i + (header.len as usize))];
                i += header.len as usize;

                let req = match Self::parse_request(header, data) {
                    Some(v) => v,
                    None => {
                        // TODO: Send back an error
                        continue;
                    }
                };

                // TODO: Keep track of these tasks.
                executor::spawn(Self::handle_request(shared.clone(), req));
            }

            if i == 0 {
                return Err(err_msg("Individual requests are too large to process"));
            }

            let remaining = buffer_len - i;
            buffer.copy_within(i..(i + remaining), 0);
            buffer_len = remaining;
        }

        Ok(())
    }

    // fuse_init_in

    fn parse_request(header: fuse_in_header, mut data: &[u8]) -> Option<Request> {
        const FUSE_INIT: u32 = fuse_opcode::FUSE_INIT as u32;

        let op = match header.opcode {
            FUSE_INIT => {
                let mut req = sys::bindings::fuse_init_in::default();
                match parse_cstruct(data, &mut req) {
                    Some(n) => data = &data[n..],
                    None => return None,
                };

                RequestOperation::Init(req)
            }
            _ => {
                eprintln!("Unsupported FUSE opcode: {}", header.opcode);
                return None;
            }
        };

        if !data.is_empty() {
            eprintln!("Unused bytes in request: {}", data.len());
            // return None;
        }

        Some(Request { header, op })

        /*
        Core operations to support:
        FUSE_MKDIR = 9,
        FUSE_UNLINK = 10,
        FUSE_RMDIR = 11,
        FUSE_RENAME = 12,
        FUSE_OPEN = 14,
        FUSE_READ = 15,
        FUSE_WRITE = 16,
        FUSE_RELEASE = 18,
        FUSE_FSYNC = 20,
        FUSE_FLUSH = 25,
        FUSE_INIT = 26,
        FUSE_OPENDIR = 27,
        FUSE_READDIR = 28,
        FUSE_RELEASEDIR = 29,
        FUSE_FSYNCDIR = 30,
        FUSE_RENAME2 = 45,
        FUSE_FALLOCATE = 43,
        FUSE_ACCESS = 34,
        FUSE_CREATE = 35,
        */

        /*
        Other operaitons:
        FUSE_LOOKUP = 1,
        FUSE_FORGET = 2,
        FUSE_GETATTR = 3,
        FUSE_SETATTR = 4,
        FUSE_READLINK = 5,
        FUSE_SYMLINK = 6,
        FUSE_MKNOD = 8,
        FUSE_LINK = 13,
        FUSE_STATFS = 17,
        FUSE_SETXATTR = 21,
        FUSE_GETXATTR = 22,
        FUSE_LISTXATTR = 23,
        FUSE_REMOVEXATTR = 24,
        FUSE_GETLK = 31,
        FUSE_SETLK = 32,
        FUSE_SETLKW = 33,
        FUSE_INTERRUPT = 36,
        FUSE_BMAP = 37,
        FUSE_DESTROY = 38,
        FUSE_IOCTL = 39,
        FUSE_POLL = 40,
        FUSE_NOTIFY_REPLY = 41,
        FUSE_BATCH_FORGET = 42,
        FUSE_READDIRPLUS = 44,
        FUSE_LSEEK = 46,
        FUSE_COPY_FILE_RANGE = 47,
        FUSE_SETUPMAPPING = 48,
        FUSE_REMOVEMAPPING = 49,
        FUSE_SYNCFS = 50,
        CUSE_INIT = 4096,
        */
    }

    async fn handle_request(shared: Arc<Shared>, request: Request) {
        println!("{:#?}", request);

        //
    }

    async fn send_error_response(
        shared: &Shared,
        header: &fuse_in_header,
        error: Errno,
    ) -> Result<()> {
        let mut out = vec![];

        let mut out_header = fuse_out_header::default();
        out_header.len = core::mem::size_of::<fuse_out_header>() as u32;
        out_header.unique = header.unique;
        // TODO: Check this.
        out_header.error = -(error.0 as i32);

        out.extend_from_slice(serialize_cstruct(&out_header));

        Self::send_raw_response(shared, &out).await?;

        Ok(())
    }

    async fn send_raw_response(shared: &Shared, data: &[u8]) -> Result<()> {
        let n = shared.file.write_at(0, data).await?;
        // We make the assumption that this will never happen similar to libfuse.
        if n != data.len() {
            return Err(err_msg("Did not response in one atomic operation"));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Request {
    pub header: fuse_in_header,
    pub op: RequestOperation,
}

#[derive(Debug)]
pub enum RequestOperation {
    Init(sys::bindings::fuse_init_in),
}

// TODO: Make this unsafe.
fn parse_cstruct<T>(input: &[u8], out: &mut T) -> Option<usize> {
    let size = core::mem::size_of::<T>();
    if input.len() < size {
        return None;
    }

    let out_slice =
        unsafe { core::slice::from_raw_parts_mut(core::mem::transmute::<_, *mut u8>(out), size) };
    out_slice.copy_from_slice(&input[0..size]);

    Some(size)
}

// TODO: Make this unsafe.
fn serialize_cstruct<'a, T>(input: &'a T) -> &'a [u8] {
    let size = core::mem::size_of::<T>();
    unsafe { core::slice::from_raw_parts(core::mem::transmute::<_, *const u8>(input), size) }
}
