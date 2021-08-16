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

const USER_NS_SETUP_BYTE: u8 = 0x41;
const TERMINAL_FD_BYTE: u8 = 0x55;
const FINISHED_SETUP_BYTE: u8 = 0x52;

/// Between every
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
    pub fn notify_user_ns_setup(&mut self) -> Result<()> {
        self.socket.write_all(&[USER_NS_SETUP_BYTE])?;
        Ok(())
    }

    /// NOTE: This uses asserts that should never fail given the amount of
    /// memory we have allocated. If one of the assertions does fail, then
    /// that means that we may be leaking
    pub fn recv_terminal_fd(&mut self) -> Result<std::fs::File> {
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
            return Err(err_msg("Child hung up before receiving termainal fd."));
        }

        if buf[0] != TERMINAL_FD_BYTE {
            return Err(err_msg(
                "Received incorrect byte while waiting for terminal fd",
            ));
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

    pub fn notify_finished(&mut self) -> Result<()> {
        self.socket.write_all(&[FINISHED_SETUP_BYTE])?;
        Ok(())
    }
}

pub struct SetupSocketChild {
    socket: std::fs::File,
}

impl SetupSocketChild {
    pub fn wait_user_ns_setup(&mut self) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)?;
        if buf[0] != USER_NS_SETUP_BYTE {
            return Err(err_msg("Expected user ns setup byte"));
        }

        Ok(())
    }

    pub fn send_terminal_fd(&mut self, file: std::fs::File) -> Result<()> {
        let data = [TERMINAL_FD_BYTE; 1];

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

    pub fn wait_finished(&mut self) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)?;
        if buf[0] != FINISHED_SETUP_BYTE {
            return Err(err_msg("Expected finished setup byte"));
        }

        Ok(())
    }
}
