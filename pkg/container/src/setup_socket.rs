use std::{
    io::{Read, Write},
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
};

use common::errors::*;

/// Double ended UNIX socket used to communicate between a parent and child
/// process during setup of the child process.
pub struct SetupSocket {}

// TODO: Update this code to not use 'nix'
impl SetupSocket {
    pub fn create() -> Result<(SetupSocketParent, SetupSocketChild)> {
        let (mut socket_a, mut socket_b) = unsafe {
            sys::socketpair(
                sys::AddressFamily::AF_UNIX,
                sys::SocketType::SOCK_STREAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::NONE,
            )?
        };

        Ok((
            SetupSocketParent {
                socket: socket_a.into(),
            },
            SetupSocketChild {
                socket: socket_b.into(),
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
        self.socket.write_all(&[event_id])
            .map_err(|e| format_err!("SetupSocketParent::notify({}) failed: {}", event_id, e))?;
        Ok(())
    }

    pub fn wait(&mut self, event_id: u8) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)
            .map_err(|e| format_err!("SetupSocketParent::wait({}) failed: {}", event_id, e))?;
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

        let data = [sys::IoSliceMut::new(&mut buf[..])];

        let mut messages =
            sys::ControlMessageBuffer::new(&[sys::ControlMessage::ScmRights(vec![0, 0, 0, 0, 0])]);
        
        let mut msg = sys::MessageHeaderMut::new(&data[..], None, Some(&mut messages));

        // TODO: build the msg internally and return decoupled parts so that msg doesn't need to hold ownership over all the buffers.
        let n = sys::recvmsg(self.socket.as_raw_fd(), &mut msg, sys::bindings::MSG_CMSG_CLOEXEC as u32 as i32)
            .map_err(|e| format_err!("SetupSocketParent::recv_fd({}) failed: {}", event_id, e))?;
        if n == 0 {
            return Err(err_msg("Child hung up before receiving fd."));
        }

        if msg.data()[0].as_ref()[0] != event_id {
            return Err(err_msg("Received wrong event while waiting for fd"));
        }

        let mut msg_iter = msg.control_messages().unwrap();

        let file = match msg_iter.next() {
            Some(sys::ControlMessage::ScmRights(fds)) => {
                assert_eq!(fds.len(), 1);
                unsafe { std::fs::File::from_raw_fd(fds[0]) }
            }
            _ => {
                return Err(err_msg("Unexpected to receive an fd"));
            }
        };

        assert!(msg_iter.next().is_none());

        Ok(file)
    }
}

pub struct SetupSocketChild {
    socket: std::fs::File,
}

impl SetupSocketChild {
    pub fn notify(&mut self, event_id: u8) -> Result<()> {
        self.socket.write_all(&[event_id])
            .map_err(|e| format_err!("SetupSocketChild::notify({}) failed: {}", event_id, e))?;
        Ok(())
    }

    pub fn wait(&mut self, event_id: u8) -> Result<()> {
        let mut buf = [0u8; 1];
        self.socket.read_exact(&mut buf)
            .map_err(|e| format_err!("SetupSocketChild::wait({}) failed: {}", event_id, e))?;
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

        let data_slices = [sys::IoSlice::new(&data[..])];

        let messages =
            sys::ControlMessageBuffer::new(&[sys::ControlMessage::ScmRights(vec![file.as_raw_fd()])]);

        let msg = sys::MessageHeader::new(&data_slices[..], None, Some(&messages));

        let _ = sys::sendmsg(self.socket.as_raw_fd(), &msg, 0)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() -> Result<()> {

        let (mut parent, mut child) = SetupSocket::create()?;

        let (pipe_reader, mut pipe_writer) = sys::pipe2(sys::O_CLOEXEC)?;

        let mut pipe_writer = std::fs::File::from(pipe_writer);

        parent.notify(1)?;
        child.wait(1)?;

        child.notify(2)?;
        parent.wait(2)?;

        child.send_fd(3, pipe_reader.into())?;

        let mut new_pipe_reader = parent.recv_fd(3)?;

        pipe_writer.write_all(b"HELLO")?;

        let mut out = [0u8; 20];
        let n = new_pipe_reader.read(&mut out[..])?;
        assert_eq!(&out[..n], b"HELLO");

        Ok(())
    }

}
