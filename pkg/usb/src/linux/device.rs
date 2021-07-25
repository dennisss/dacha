/*

Transfer cancellation:
- Transfers are cancelled once their corresponding futures are dropped.

Memory Management Notes:
- We give the linux kernel a pointer to:
    - the USBDEV URB
    - The read/write buffer
    - The DeviceTransfer object (in the usrcontext field of the URB)
- Additionally the Context's background thread will be returned pointers to the above
  via polling syscalls
- So we need to ensure that all the above pointers point to valid memory while:
    - The linux kernel is processing the URB
    - The Context background thread may still be accessing one of them
- We use an DeviceTransfer objects to own the URB and read/write buffer memory.
- All of the DeviceTransfer objects are owned by the DeviceState in a big list.

- So to ensure that the DeviceTransfer is alive for the right amount of time, we need to consider
    1. When are DeviceTransfers removed/dropped from the list in the DeviceState
        - They are only removed individually by the Context background thread after it has
          received a completion notification from the linux kernel and is done processing it.
        - NOTE: DeviceTransfers are added to the big list in the DeviceState under a mutex lock before
          the URB is submitted to ensure that the background thread can't look at it too early.
        - NOTE: We assume that the linux kernel will never return us the same URB reference twice
          via a REAPURB ioctl.
    2. When is the DeviceState object dropped.
       We implement a custom Drop handler for the DeviceState which does the following:
       1. closes the file descriptor.
         - NOTE: We assume that when we close() the file descriptor for usbdevfs, the linux
           kernel will stop all future writes to the user memory referenced in the URB and
           future ioctl calls will not reap any URBs from this file.
       2. Waits for the context background thread to iterate through at least one polling cycle.
          - This ensures that it finishes processing any URBs that it received before we closed
            the file.
       3. Only after the above two are done does the memory associated with the DeviceTransfers get
          deleted.

Other Assumptions that we make:
- If the ioctl with SUBMITURB fails, then the failure is atomic and we will never be able to reap the URB given to it.
- If the DISCARDURB command succeedes, then it will be followed by the URB being reaped either successfully or with an error.

*/

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use common::async_std::fs;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::async_std::channel;
use common::task::ChildTask;
use common::{async_std::path::Path, errors::*, futures::StreamExt};

use crate::language::Language;
use crate::endpoint::*;
use crate::descriptors::*;
use crate::linux::usbdevfs::*;
use crate::linux::context::*;
use crate::linux::transfer::*;



/// Handle to a single open USB device.
///
/// All operations are thread safe. After you are done with the device, call .close() to gracefully
/// close the device. 
///
/// The device will stay open until this object is dropped.
/// The Context used to open this device similarly won't be destroyed until all Device objects
/// associated with it are dropped. 
pub struct Device {
    pub(crate) id: usize,

    pub(crate) device_descriptor: DeviceDescriptor,
    pub(crate) endpoint_descriptors: HashMap<u8, EndpointDescriptor>, 

    /// Context that was used to open this device.
    /// NOTE: As the Context manages the background events thread, it must live longer than the
    /// device, otherwise most of the Device methods will be broken.
    pub(crate) context: Arc<Context>,

    pub(crate) state: Arc<DeviceState>,

    pub(crate) closed: bool
}

/// NOTE: The cleanup of data in this struct is handled by Device::drop().
pub(crate) struct DeviceState {
    pub(crate) bus_num: usize,
    pub(crate) dev_num: usize,

    pub(crate) fd: libc::c_int,

    /// All pending transfers on the this device.
    /// This is the primary owner of the DeviceTransfers.
    ///
    /// Values are ONLY removed from this when the URB is reaped (or after we close the fd).
    /// 
    pub(crate) transfers: std::sync::Mutex<DeviceStateTransfers>,
}

#[derive(Default)]
pub(crate) struct DeviceStateTransfers {
    pub last_id: usize,

    // TODO: There is no point in making these Arcs as we always own them.
    // It would probably be simpler to give back the use a separate transfer object that just
    // references the transfer if id.
    pub active: HashMap<usize, Arc<DeviceTransferState>>
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.closed {
            // TODO: Print error?
            self.close_impl();
        }
    }
}

impl Device {
    /// Closes the device.
    /// Any pending transfers will be immediately cancelled.
    ///
    /// NOTE: If not called manaully, this will also run on Drop, but you won't know if
    /// it was successful.
    pub fn close(mut self) -> Result<()> {
        self.close_impl()
    }

    fn close_impl(&mut self) -> Result<()> {
        self.closed = true;

        // Remove the device from the context so that the background thread stops listening for events.
        self.context.remove_device(self.id)?;

        // Wait for the background thread to perform at least one cycle (to ensure that it is no lpnger waiting on the device)
        // TODO:

        // TODO: Check return value.
        unsafe { libc::close(self.state.fd) };

        // Wait until another background thread cycle passes so that we know that the background
        // thread isn't working on anything related to our fd.
        //
        // This is important to ensure that all the transfers memory can be safely dropped.
        let waiter = self.context.add_background_thread_waiter();
        self.context.notify_background_thread()?;
        let _ = waiter.recv();
        

        // Notify all pending transfers that the device is closed.
        let mut transfers = self.state.transfers.lock().unwrap();
        for (_, transfer) in transfers.active.iter() {
            let _ = transfer.sender.try_send(Err(crate::ErrorKind::DeviceClosing));
        }
        transfers.active.clear();

        // NOTE: At this point, there may still be references to the DeviceState in
        // DeviceTransfer objects if the 
        // if Arc::strong_count(&self.state) != 1 {
        //     return Err(err_msg("Stranded references to DeviceState"));
        // }

        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        unsafe { usbdevfs_reset(self.state.fd) }?;
        Ok(())
    }

    pub fn set_active_configuration(&self, index: u8) -> Result<()> {
        let mut data = index as libc::c_uint;
        unsafe { usbdevfs_setconfiguration(self.state.fd, &mut data) }?;
        Ok(())
    }

    pub fn kernel_driver_active(&self, interface: u8) -> Result<bool> {
        let mut driver = usbdevfs_getdriver {
            interface: interface as libc::c_uint,
            driver: [0; 256]
        };

        // TODO: ENODATA if no driver is present?
        let r = unsafe { usbdevfs_getdriver_fn(self.state.fd, &mut driver) };
        match r {
            Ok(_) => {},
            // TODO: Check this.
            Err(nix::Error::Sys(nix::errno::Errno::ENODATA)) => { return Ok(false); }
            Err(e) => { return Err(e.into()); }
        };

        let mut null_index = 0;
        while null_index < driver.driver.len() {
            if driver.driver[null_index] == 0 {
                break;
            }

            null_index += 1;
        }

        let driver_name = std::str::from_utf8(&driver.driver[0..null_index])?;
        Ok(driver_name != "usbfs")
    }

    pub fn detach_kernel_driver(&self, interface: u8) -> Result<()> {

        let mut command = usbdevfs_ioctl {
            ifno: interface as libc::c_int,
            ioctl_code: USBDEVFS_IOC_DISCONNECT,
            data: 0
        };

        unsafe { usbdevfs_ioctl_fn(self.state.fd, &mut command) }?;

        Ok(())
    }

    pub fn claim_interface(&mut self, number: u8) -> Result<()> {
        let data = number as libc::c_uint;
        unsafe { usbdevfs_claim_interface(self.state.fd, std::mem::transmute(&data)) }?;
        Ok(())
    }

    pub fn release_interface(&mut self, number: u8) -> Result<()> {
        let data = number as libc::c_uint;
        unsafe { usbdevfs_release_interface(self.state.fd, std::mem::transmute(&data)) }?;
        Ok(())
    }

    pub fn set_alternate_setting(&self, interface: u8, setting: u8) -> Result<()> {
        let mut data = usbdevfs_setinterface {
            interface: interface as libc::c_uint,
            altsetting: setting as libc::c_uint,
        };

        unsafe { usbdevfs_setinterface_fn(self.state.fd, &mut data) }?;

        Ok(())
    }

    fn start_transfer(&self, typ: u8, endpoint: u8, flags: libc::c_uint, buffer: Vec<u8>) -> Result<DeviceTransfer> {
        let (sender, receiver) = channel::bounded(1);

        let mut transfers = self.state.transfers.lock().unwrap();

        let id = transfers.last_id + 1;
        transfers.last_id = id;

        let mut transfer = Arc::new(DeviceTransferState {
            id,
            device_state: self.state.clone(),
            urb: usbdevfs_urb {
                typ,
                endpoint,
                status: 0,
                flags,
                buffer: unsafe { std::mem::transmute::<&u8, _>(&buffer[0]) },
                buffer_length: buffer.len() as libc::c_int,
                actual_length: 0,
                start_frame: 0,
                stream_id: 0,
                error_count: 0,
                signr: 0,
                usrcontext: unsafe { std::mem::transmute(&0) },
            },
            buffer,
            sender,
            receiver
        });

        unsafe {
            let mut transfer_mut = Arc::get_mut(&mut transfer).unwrap();

            // Set the URB usrcontext to a reference to the DeviceTransfer itself.
            transfer_mut.urb.usrcontext =
                std::mem::transmute::<&mut DeviceTransferState, _>(transfer_mut);

            // Submit it!
            usbdevfs_submiturb(self.state.fd, &mut transfer_mut.urb)?;
        }

        transfers.active.insert(id, transfer.clone());

        Ok(DeviceTransfer { state: transfer })
    }

    // TODO: Need to check the direction bit in the packet.
    pub async fn write_control(&self, pkt: SetupPacket, data: &[u8]) -> Result<()> {
        let pkt_size = std::mem::size_of::<SetupPacket>();
        let mut buffer = vec![0u8; pkt_size + data.len()];

        buffer[0..pkt_size].copy_from_slice(
            unsafe { std::slice::from_raw_parts(std::mem::transmute(&pkt), pkt_size) }
        );

        buffer[pkt_size..].copy_from_slice(data);
    
        let transfer = self.start_transfer(
            USBDEVFS_URB_TYPE_CONTROL, CONTROL_ENDPOINT, 0, buffer)?;


        transfer.wait().await?;

        if transfer.state.urb.actual_length != (data.len() as i32) {
            return Err(err_msg("Not all data was written"));
        }

        Ok(())
    }

    pub async fn read_control(&self, pkt: SetupPacket, data: &mut [u8]) -> Result<usize> {
        let pkt_size = std::mem::size_of::<SetupPacket>();
        let mut buffer = vec![0u8; pkt_size + data.len()];

        buffer[0..pkt_size].copy_from_slice(
            unsafe { std::slice::from_raw_parts(std::mem::transmute(&pkt), pkt_size) }
        );

        let transfer = self.start_transfer(
            USBDEVFS_URB_TYPE_CONTROL, CONTROL_ENDPOINT, 0, buffer)?;


        transfer.wait().await?;

        let n = transfer.state.urb.actual_length as usize;
        let received_data = &transfer.state.buffer[pkt_size..(pkt_size + n)];
        data[0..n].copy_from_slice(received_data);
        Ok(n)
    }

    async fn read_string_raw(&self, index: u8, lang_id: u16) -> Result<Vec<u8>> {
        let pkt_size = std::mem::size_of::<SetupPacket>();


        // 256 is larger than the maximum descriptor size.
        let mut buffer = vec![0u8; 256];

        let pkt = SetupPacket {
            bmRequestType: 0b10000000,
            bRequest: StandardRequestType::GET_DESCRIPTOR as u8,
            wValue: ((DescriptorType::STRING as u16) << 8) | (index as u16),
            wIndex: lang_id,
            wLength: (buffer.len() - pkt_size) as u16, // TODO: Check this.
        };

        let nread = self.read_control(pkt, &mut buffer).await?;
    
        let received_data = &buffer[0..nread];

        // TODO: Check for buffer overflows
        let blen = received_data[0] as usize;
        if blen != received_data.len() {
            // Most likely this means that we didn't provide a big enough buffer.
            return Err(err_msg("Bad len"));
        }

        if received_data[1] != (DescriptorType::STRING as u8) {
            return Err(err_msg("Got wrong descriptor type"));
        }

        // TODO: Check if we overflowed the size of the buffer.

        Ok(received_data[2..].to_vec())
    }

    pub async fn read_languages(&self) -> Result<Vec<Language>> {
        let data = self.read_string_raw(0, 0).await?;
        if (data.len() % 2) != 0 {
            return Err(err_msg("Languages index string has invalid size"));
        }

        let mut out = vec![];
        for i in 0..(data.len() / 2) {
            let id = u16::from_le_bytes(*array_ref![data, 2*i, 2]);
            out.push(Language::from_id(id));
        }

        Ok(out)
    }

    pub async fn read_string(&self, index: u8, language: Language) -> Result<String> {
        let data = self.read_string_raw(index, language.id()).await?;
        Ok(String::from_utf8(data)?)
    }

    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.device_descriptor
    }

    pub async fn read_manufacturer_string(&self, language: Language) -> Result<String> {
        self.read_string(self.device_descriptor.iManufacturer, language).await
    }
    
    pub async fn read_product_string(&self, language: Language) -> Result<String> {
        self.read_string(self.device_descriptor.iProduct, language).await
    }

    pub async fn read_interrupt(&self, endpoint: u8, buffer: &mut [u8]) -> Result<usize> {
        check_can_read_endpoint(endpoint)?;

        // TODO: Verify that the endpoint has a descriptor?

        let buf = vec![0u8; buffer.len()];
        let transfer = self.start_transfer(USBDEVFS_URB_TYPE_INTERRUPT, endpoint, 0, buf)?;

        transfer.wait().await?;

        let n = transfer.state.urb.actual_length as usize;

        if n > buffer.len() {
            return Err(crate::Error {
                kind: crate::ErrorKind::Overflow,
                message: "Too many bytes read".into()
            }.into());
        }

        buffer[0..n].copy_from_slice(&transfer.state.buffer[0..n]);

        Ok(n)
    }

    pub async fn write_interrupt(&self, endpoint: u8, buffer: &[u8]) -> Result<()> {
        check_can_write_endpoint(endpoint)?;

        let endpoint_desc = self.endpoint_descriptors.get(&endpoint)
            .ok_or_else(|| err_msg("Missing descriptor for endpoint"))?;
        
        // TODO: Check the behavior of linux in this case. It will likely try to split the transfer into multiple parts?
        if buffer.len() > (endpoint_desc.wMaxPacketSize as usize) {
            return Err(err_msg("Interrupt write larger than max packet size"));
        }

        // TODO: If we try to send more data than the max_packet_size or interrupt size limit., most devices will just break.
        

        let buf = buffer.to_vec();
        let transfer = self.start_transfer(USBDEVFS_URB_TYPE_INTERRUPT, endpoint, 0, buf)?;

        transfer.wait().await?;

        if transfer.state.urb.actual_length as usize != buffer.len() {
            return Err(err_msg("Not all bytes were written"));
        }

        Ok(())
    }

}