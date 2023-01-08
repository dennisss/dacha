use std::{
    io::{Read, Write},
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
};

use common::errors::*;
use nix::sys::{
    socket::{
        recvmsg, sendmsg, socketpair, AddressFamily, ControlMessage, ControlMessageOwned, MsgFlags,
        SockFlag, SockType,
    },
    uio::IoVec,
};

/// Double ended UNIX socket used to communicate between a parent and child
/// process during setup of the child process.
pub struct SetupSocket {}

impl SetupSocket {
    pub fn create() -> Result<(SetupSocketParent, SetupSocketChild)> {
        let (socket_a, socket_b) = socketpair(
            AddressFamily::Unix,
            SockType::Stream,
            None,
            SockFlag::SOCK_CLOEXEC,
        )?;

        Ok((
            SetupSocketParent {
                socket: unsafe { std::fs::File::from_raw_fd(socket_a) },
            },
            SetupSocketChild {
                socket: unsafe { std::fs::File::from_raw_fd(socket_b) },
            },
        ))
    }
}

/// TODO: Make the parent interface fully async.
pub struct SetupSocketParent {
    socket: std::fs::File,
}

impl SetupSocketParent {
    pub fn notify(&mut self, event_id: u8) -> Result<()> {
        self.socket.write_all(&[event_id])?;
        Ok(())
    }

    pub fn wait(&mut self, event_id: u8) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)?;
        if buf[0] != event_id {
            return Err(format_err!(
                "Expected event {:x} but got {:x}",
                event_id,
                buf[0]
            ));
        }

        Ok(())
    }

    /// NOTE: This uses asserts that should never fail given the amount of
    /// memory we have allocated. If one of the assertions does fail, then
    /// that means that we may be leaking files.
    pub fn recv_fd(&mut self, event_id: u8) -> Result<std::fs::File> {
        let mut buf = [0u8; 1];

        let mut cmsg_buffer = nix::cmsg_space!(RawFd);

        // TODO: Make this non-blocking.
        let msg = recvmsg(
            self.socket.as_raw_fd(),
            &[IoVec::from_mut_slice(&mut buf)],
            Some(&mut cmsg_buffer),
            MsgFlags::MSG_CMSG_CLOEXEC,
        )?;
        if msg.bytes == 0 {
            return Err(err_msg("Child hung up before receiving fd."));
        }

        if buf[0] != event_id {
            return Err(err_msg("Received event while waiting for fd"));
        }

        let mut msg_iter = msg.cmsgs();
        let file = match msg_iter.next() {
            Some(ControlMessageOwned::ScmRights(fds)) => {
                assert_eq!(fds.len(), 1);
                unsafe { std::fs::File::from_raw_fd(fds[0]) }
            }
            _ => {
                return Err(err_msg("Unexpected to receive an fd"));
            }
        };

        assert_eq!(msg_iter.next(), None);

        Ok(file)
    }
}

pub struct SetupSocketChild {
    socket: std::fs::File,
}

impl SetupSocketChild {
    pub fn notify(&mut self, event_id: u8) -> Result<()> {
        self.socket.write_all(&[event_id])?;
        Ok(())
    }

    pub fn wait(&mut self, event_id: u8) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)?;
        if buf[0] != event_id {
            return Err(format_err!(
                "Expected event {:x} but got {:x}",
                event_id,
                buf[0]
            ));
        }

        Ok(())
    }

    pub fn send_fd(&mut self, event_id: u8, file: std::fs::File) -> Result<()> {
        let data = [event_id; 1];

        let fds = [file.as_raw_fd()];
        let control_msg = ControlMessage::ScmRights(&fds);
        let _ = sendmsg(
            self.socket.as_raw_fd(),
            &[IoVec::from_slice(&data)],
            &[control_msg],
            MsgFlags::empty(),
            None,
        )?;

        Ok(())
    }
}
