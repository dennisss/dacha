use std::sync::Arc;

use common::errors::*;
use common::async_std::channel;

use crate::linux::usbdevfs::usbdevfs_urb;
use crate::linux::device::DeviceState;

/// Represents a single ongoing USB I/O request to the linux kernel.
/// (corresponds to a single Linux USBDEVFS URB)
///
/// NOTE: The DeviceTransfer must be pinned at a static location in memory as the 'urb' is
/// referenced in kernel requests (so you'll only ever see Arc<DeviceTransfer>'s and never
/// bare ones).
pub struct DeviceTransfer {
    /// Id of this transfer. Specific to this device.
    pub(crate) id: usize,

    /// Reference to the DeviceState containing this transfer. Used for cleaning up the transfer
    /// once it is successfully reaped.
    pub(crate) device_state: Arc<DeviceState>,

    pub(crate) urb: usbdevfs_urb,

    /// Buffer which is referenced in the above URB.
    /// If this is a write request, this will either contain user data being sent to the device.
    /// Else, this will be asynchronously filled by the kernel with data received from the device. 
    pub(crate) buffer: Vec<u8>,

    /// Channel sender used for notifying the corresponding receiver that the transfer is
    /// complete (or failed). 
    pub(crate) sender: channel::Sender<DeviceTransferCompletion>,
    pub(crate) receiver: channel::Receiver<DeviceTransferCompletion>
}

impl DeviceTransfer {
    /// NOTE: It is only valid to call this once.
    pub async fn wait(&self) -> Result<()> {
        match self.receiver.recv().await? {
            DeviceTransferCompletion::Reaped => {
                if self.urb.status != 0 {
                    let errno = -1*self.urb.status;

                    // This will occur when we are performing a bulk/interrupt read and we
                    // received a packet that would overflow our receiving buffer.
                    //
                    // NOTE: This will never if the buffer size is a multiple of the maximum
                    // packet size for the endpoint.
                    if errno == libc::EOVERFLOW {
                        return Err(err_msg("Received data overflowed buffer"));
                    }

                    return Err(nix::Error::from_errno(nix::errno::from_i32(errno)).into());
                }
            }
            DeviceTransferCompletion::DeviceClosing => {
                return Err(err_msg("Device closed"));
            }
        }

        Ok(())
    }

    pub(crate) fn perform_reap(&self) {
        let _ = self.sender.try_send(DeviceTransferCompletion::Reaped);

        // Remove transfer as it's no longer needed.
        let mut transfers = self.device_state.transfers.lock().unwrap();
        transfers.active.remove(&self.id);
    }
}

pub enum DeviceTransferCompletion {
    /// The transfer was reaped normally by the Context background thread.
    /// The status of the transfer is available in urb.status.
    Reaped,

    /// The transfer was stopped because the associated device is closing.
    DeviceClosing,
}