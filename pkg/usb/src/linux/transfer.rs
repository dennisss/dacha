use std::sync::Arc;

use common::async_std::channel;
use common::errors::*;

use crate::linux::device::DeviceState;
use crate::linux::usbdevfs::{usbdevfs_discardurb, usbdevfs_urb};

// TODO: If a reference to a transfer is dropped instead of being waited on, we
// should just cancel it!

pub struct DeviceTransfer {
    pub(crate) state: Arc<DeviceTransferState>,
}

impl Drop for DeviceTransfer {
    fn drop(&mut self) {
        if let Err(e) = self.state.cancel() {
            eprintln!("Error while cancelling USB transfer: {:?}", e);
        }
    }
}

impl DeviceTransfer {
    pub async fn wait(&self) -> Result<()> {
        self.state.wait().await
    }
}

/// Represents a single ongoing USB I/O request to the linux kernel.
/// (corresponds to a single Linux USBDEVFS URB)
///
/// NOTE: The DeviceTransfer must be pinned at a static location in memory as
/// the 'urb' is referenced in kernel requests (so you'll only ever see
/// Arc<DeviceTransfer>'s and never bare ones).
pub struct DeviceTransferState {
    /// Id of this transfer. Specific to this device.
    pub(crate) id: usize,

    /// Reference to the DeviceState containing this transfer. Used for cleaning
    /// up the transfer once it is successfully reaped.
    pub(crate) device_state: Arc<DeviceState>,

    pub(crate) urb: usbdevfs_urb,

    /// Buffer which is referenced in the above URB.
    /// If this is a write request, this will either contain user data being
    /// sent to the device. Else, this will be asynchronously filled by the
    /// kernel with data received from the device.
    pub(crate) buffer: Vec<u8>,

    /// Channel sender used for notifying the corresponding receiver that the
    /// transfer is complete (or failed).
    pub(crate) sender: channel::Sender<std::result::Result<(), crate::ErrorKind>>,
    pub(crate) receiver: channel::Receiver<std::result::Result<(), crate::ErrorKind>>,
}

impl DeviceTransferState {
    /// NOTE: It is only valid to call this once.
    async fn wait(&self) -> Result<()> {
        if let Err(kind) = self.receiver.recv().await? {
            return Err(crate::Error {
                kind,
                message: String::new(),
            }
            .into());
        }

        // Error code meanings are documented here:
        // https://www.kernel.org/doc/html/latest/driver-api/usb/error-codes.html#error-codes-returned-by-in-urb-status-or-in-iso-frame-desc-n-status-for-iso
        if self.urb.status != 0 {
            let errno = -1 * self.urb.status;

            // This will occur when we are performing a bulk/interrupt read and we
            // received a packet that would overflow our receiving buffer.
            //
            // NOTE: This will never if the buffer size is a multiple of the maximum
            // packet size for the endpoint.
            if errno == libc::EOVERFLOW {
                return Err(crate::Error {
                    kind: crate::ErrorKind::Overflow,
                    message: String::new(),
                }
                .into());
            }

            if errno == libc::ENODEV || errno == libc::ESHUTDOWN {
                return Err(crate::Error {
                    kind: crate::ErrorKind::DeviceDisconnected,
                    message: String::new()
                }.into());
            }

            if errno == libc::EPROTO || errno == libc::EILSEQ {
                return Err(crate::Error {
                    kind: crate::ErrorKind::TransferFailure,
                    message: String::new()
                }.into());
            }

            if errno == libc::EPIPE {
                return Err(crate::Error {
                    kind: crate::ErrorKind::TransferStalled,
                    message: String::new()
                }.into());
            }

            return Err(nix::Error::from_errno(nix::errno::from_i32(errno)).into());
        }

        Ok(())
    }

    pub(crate) fn perform_reap(&self) {
        let _ = self.sender.try_send(Ok(()));

        // Remove transfer as it's no longer needed.
        let mut transfers = self.device_state.transfers.lock().unwrap();
        transfers.active.remove(&self.id);
    }

    /// Cancels the transfer. This will cause a current/future call to wait() to
    /// finish.
    fn cancel(&self) -> Result<()> {
        let _ = self
            .sender
            .try_send(Err(crate::ErrorKind::TransferCancelled));

        // NOTE: This will cause it to be reaped with a -2 error (so I can't delete the
        // memory yet!!)
        let res =
            unsafe { usbdevfs_discardurb(self.device_state.fd, std::mem::transmute(&self.urb)) };
        match res {
            Ok(_) => {}
            Err(nix::Error::Sys(nix::errno::Errno::EINVAL)) => {
                // In this case, the transfer was already cancelled or already
                // reaped.
            }
            // TODO: Figure out what the error code will be after the device is closed.
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(())
    }
}
