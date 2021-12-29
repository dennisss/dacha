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

use alloc::string::String;
use alloc::vec::Vec;
use std::collections::HashMap;
use std::sync::Arc;

use common::async_std::channel;
use common::errors::*;

use crate::descriptor_iter::{Descriptor, DescriptorIter};
use crate::descriptors::*;
use crate::endpoint::*;
use crate::language::Language;
use crate::linux::context::*;
use crate::linux::transfer::*;
use crate::linux::usbdevfs::*;

/// Handle to a single open USB device.
///
/// All operations are thread safe. After you are done with the device, call
/// .close() to gracefully close the device.
///
/// The device will stay open until this object is dropped.
/// The Context used to open this device similarly won't be destroyed until all
/// Device objects associated with it are dropped.
pub struct Device {
    id: usize,

    descriptors: Vec<Descriptor>,
    device_descriptor: DeviceDescriptor,
    endpoint_descriptors: HashMap<u8, EndpointDescriptor>,

    /// Context that was used to open this device.
    /// NOTE: As the Context manages the background events thread, it must live
    /// longer than the device, otherwise most of the Device methods will be
    /// broken.
    context_state: Arc<ContextState>,

    state: Arc<DeviceState>,

    closed: bool,
}

/// NOTE: The cleanup of data in this struct is handled by Device::drop().
pub(crate) struct DeviceState {
    pub(crate) bus_num: usize,
    pub(crate) dev_num: usize,

    pub(crate) fd: libc::c_int,
    pub(crate) fd_closed: std::sync::Mutex<bool>,

    pub(crate) has_error: std::sync::Mutex<bool>,

    /// All pending transfers on the this device.
    /// This is the primary owner of the DeviceTransfers.
    ///
    /// Values are ONLY removed from this when the URB is reaped (or after we
    /// close the fd).
    pub(crate) transfers: std::sync::Mutex<DeviceStateTransfers>,
}

impl DeviceState {
    fn close_fd(&self) {
        let mut closed = self.fd_closed.lock().unwrap();
        if !*closed {
            // TODO: Check return value.
            unsafe { libc::close(self.fd) };
            *closed = true;
        }
    }
}

impl Drop for DeviceState {
    fn drop(&mut self) {
        // NOTE: This will only have an effect when the DeviceState is dropped before
        // the Device object was constructed.
        self.close_fd();
    }
}

#[derive(Default)]
pub(crate) struct DeviceStateTransfers {
    pub last_id: usize,

    // TODO: There is no point in making these Arcs as we always own them.
    // It would probably be simpler to give back the use a separate transfer object that just
    // references the transfer if id.
    pub active: HashMap<usize, Arc<DeviceTransferState>>,
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.closed {
            // TODO: Print error?
            if let Err(err) = self.close_impl() {
                eprintln!("usb::Device failed to close: {}", err);
            }
        }
    }
}

impl Device {
    pub(crate) fn create(
        context_state: Arc<ContextState>,
        state: Arc<DeviceState>,
        raw_descriptors: &[u8],
    ) -> Result<Self> {
        let mut descriptors = vec![];
        for desc in DescriptorIter::new(&raw_descriptors) {
            descriptors.push(desc?);
        }

        // TODO: Deduplicate with device_descriptor function in the Context class.
        let device_descriptor = match descriptors.first() {
            Some(Descriptor::Device(d)) => Ok(d.clone()),
            _ => Err(err_msg(
                "Expected first cached descriptor to be a device descriptor",
            )),
        }?;

        let mut endpoint_descriptors = HashMap::new();
        for desc in &descriptors {
            match desc {
                Descriptor::Endpoint(e) => {
                    if endpoint_descriptors
                        .insert(e.bEndpointAddress, e.clone())
                        .is_some()
                    {
                        return Err(err_msg("Device advertising duplicate endpoint addresses"));
                    }
                }
                _ => {}
            }
        }

        // NOTE: This must run at the very end as we need to ensure that the Device
        // object is always constructed when the device is added. Otherwise
        // remove_device() won't be called on Drop of the Device.
        let id = context_state.add_device(state.clone())?;

        Ok(Self {
            id,
            descriptors,
            device_descriptor,
            endpoint_descriptors,
            context_state,
            state,
            closed: false,
        })
    }

    /// Closes the device.
    /// Any pending transfers will be immediately cancelled.
    ///
    /// NOTE: If not called manaully, this will also run on Drop, but you won't
    /// know if it was successful.
    pub fn close(mut self) -> Result<()> {
        self.close_impl()
    }

    fn close_impl(&mut self) -> Result<()> {
        self.closed = true;

        // Remove the device from the context so that the background thread stops
        // listening for events.
        self.context_state.remove_device(self.id)?;

        // Close the file. This ensures that linux releases references to any still
        // pending transfers.
        //
        // Must occur after the remove_device() call to ensure that the context doesn't
        // try polling the file more than once.
        self.state.close_fd();

        // Wait until another background thread cycle passes so that we know that the
        // background thread isn't working on anything related to our fd.
        //
        // This is important to ensure that all the transfers memory can be safely
        // dropped.
        let waiter = self.context_state.add_background_thread_waiter();
        self.context_state.notify_background_thread()?;
        let _ = waiter.recv();

        // Notify all pending transfers that the device is closed.
        let mut transfers = self.state.transfers.lock().unwrap();
        for (_, transfer) in transfers.active.iter() {
            let _ = transfer.sender.try_send(Err(crate::Error::DeviceClosing));
        }
        transfers.active.clear();

        // NOTE: At this point, there may still be references to the DeviceState in
        // DeviceTransfer objects if the
        // if Arc::strong_count(&self.state) != 1 {
        //     return Err(err_msg("Stranded references to DeviceState"));
        // }

        Ok(())
    }

    pub fn descriptors(&self) -> &[Descriptor] {
        &self.descriptors
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
            driver: [0; 256],
        };

        // TODO: ENODATA if no driver is present?
        let r = unsafe { usbdevfs_getdriver_fn(self.state.fd, &mut driver) };
        match r {
            Ok(_) => {}
            // TODO: Check this.
            Err(nix::Error::Sys(nix::errno::Errno::ENODATA)) => {
                return Ok(false);
            }
            Err(e) => {
                return Err(e.into());
            }
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
            data: 0,
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

    fn start_transfer(
        &self,
        typ: u8,
        endpoint: u8,
        flags: libc::c_uint,
        buffer: Vec<u8>,
    ) -> Result<DeviceTransfer> {
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
                buffer: if !buffer.is_empty() {
                    unsafe { std::mem::transmute::<&u8, _>(&buffer[0]) }
                } else {
                    0
                },
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
            receiver,
        });

        unsafe {
            let mut transfer_mut = Arc::get_mut(&mut transfer).unwrap();

            // Set the URB usrcontext to a reference to the DeviceTransfer itself.
            transfer_mut.urb.usrcontext =
                std::mem::transmute::<&mut DeviceTransferState, _>(transfer_mut);

            // Submit it!
            // Error code meanings are documented here:
            // https://www.kernel.org/doc/html/latest/driver-api/usb/error-codes.html#error-codes-returned-by-usb-submit-urb
            match usbdevfs_submiturb(self.state.fd, &mut transfer_mut.urb) {
                Ok(_) => {}
                Err(nix::Error::Sys(nix::errno::Errno::ENODEV)) => {
                    return Err(crate::Error::DeviceDisconnected.into());
                }
                Err(nix::Error::Sys(nix::errno::Errno::ENOENT)) => {
                    return Err(crate::Error::EndpointNotFound.into());
                }
                Err(e) => {
                    return Err(e.into());
                }
            };
        }

        transfers.active.insert(id, transfer.clone());

        Ok(DeviceTransfer { state: transfer })
    }

    // TODO: Need to check the direction bit in the packet.
    pub async fn write_control(&self, pkt: SetupPacket, data: &[u8]) -> Result<()> {
        let pkt_size = std::mem::size_of::<SetupPacket>();
        let mut buffer = vec![0u8; pkt_size + data.len()];

        buffer[0..pkt_size].copy_from_slice(unsafe {
            std::slice::from_raw_parts(std::mem::transmute(&pkt), pkt_size)
        });

        buffer[pkt_size..].copy_from_slice(data);

        let transfer =
            self.start_transfer(USBDEVFS_URB_TYPE_CONTROL, CONTROL_ENDPOINT, 0, buffer)?;

        transfer.wait().await?;

        if transfer.state.urb.actual_length != (data.len() as i32) {
            return Err(err_msg("Not all data was written"));
        }

        Ok(())
    }

    pub async fn read_control(&self, pkt: SetupPacket, data: &mut [u8]) -> Result<usize> {
        let pkt_size = std::mem::size_of::<SetupPacket>();
        let mut buffer = vec![0u8; pkt_size + data.len()];

        buffer[0..pkt_size].copy_from_slice(unsafe {
            std::slice::from_raw_parts(std::mem::transmute(&pkt), pkt_size)
        });

        let transfer =
            self.start_transfer(USBDEVFS_URB_TYPE_CONTROL, CONTROL_ENDPOINT, 0, buffer)?;

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
            let id = u16::from_le_bytes(*array_ref![data, 2 * i, 2]);
            out.push(Language::from_id(id));
        }

        Ok(out)
    }

    pub async fn read_string(&self, index: u8, language: Language) -> Result<String> {
        let data = self.read_string_raw(index, language.id()).await?;

        if (data.len() % 2) != 0 {
            return Err(err_msg("Expected string to be in 16-bit aligned size"));
        }

        let mut out = vec![];
        for i in 0..(data.len() / 2) {
            let id = u16::from_le_bytes(*array_ref![data, 2 * i, 2]);
            out.push(id);
        }

        Ok(String::from_utf16(&out)?)
    }

    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.device_descriptor
    }

    pub async fn read_manufacturer_string(&self, language: Language) -> Result<String> {
        self.read_string(self.device_descriptor.iManufacturer, language)
            .await
    }

    pub async fn read_product_string(&self, language: Language) -> Result<String> {
        self.read_string(self.device_descriptor.iProduct, language)
            .await
    }

    pub async fn read_serial_number_string(&self, language: Language) -> Result<String> {
        self.read_string(self.device_descriptor.iSerialNumber, language)
            .await
    }

    async fn read_impl(&self, typ: u8, endpoint: u8, buffer: &mut [u8]) -> Result<usize> {
        check_can_read_endpoint(endpoint)?;

        // TODO: Verify that the endpoint has a descriptor?

        let buf = vec![0u8; buffer.len()];
        let transfer = self.start_transfer(typ, endpoint, 0, buf)?;

        transfer.wait().await?;

        let n = transfer.state.urb.actual_length as usize;

        if n > buffer.len() {
            return Err(crate::Error::Overflow.into());
        }

        buffer[0..n].copy_from_slice(&transfer.state.buffer[0..n]);

        Ok(n)
    }

    pub async fn read_interrupt(&self, endpoint: u8, buffer: &mut [u8]) -> Result<usize> {
        self.read_impl(USBDEVFS_URB_TYPE_INTERRUPT, endpoint, buffer)
            .await
    }

    pub async fn read_bulk(&self, endpoint: u8, buffer: &mut [u8]) -> Result<usize> {
        self.read_impl(USBDEVFS_URB_TYPE_BULK, endpoint, buffer)
            .await
    }

    pub async fn write_interrupt(&self, endpoint: u8, buffer: &[u8]) -> Result<()> {
        // TODO: Verify that this is an interrupt endpoint type.

        check_can_write_endpoint(endpoint)?;

        let endpoint_desc = self
            .endpoint_descriptors
            .get(&endpoint)
            .ok_or_else(|| Error::from(crate::Error::EndpointNotFound))?;

        // TODO: Check the behavior of linux in this case. It will likely try to split
        // the transfer into multiple parts?
        if buffer.len() > (endpoint_desc.wMaxPacketSize as usize) {
            return Err(err_msg("Interrupt write larger than max packet size"));
        }

        // TODO: If we try to send more data than the max_packet_size or interrupt size
        // limit., most devices will just break.

        let buf = buffer.to_vec();
        let transfer = self.start_transfer(USBDEVFS_URB_TYPE_INTERRUPT, endpoint, 0, buf)?;

        transfer.wait().await?;

        if transfer.state.urb.actual_length as usize != buffer.len() {
            return Err(err_msg("Not all bytes were written"));
        }

        Ok(())
    }

    /// NOTE: This assumes that the device protocol has some alternative way of
    /// determining the full length of the transfer as this function will not
    /// send Zero Length Packets until buffer.len() == 0.
    pub async fn write_bulk(&self, endpoint: u8, buffer: &[u8]) -> Result<()> {
        // TODO: Verify that this is an bulk endpoint type.

        check_can_write_endpoint(endpoint)?;

        let buf = buffer.to_vec();
        let transfer = self.start_transfer(USBDEVFS_URB_TYPE_BULK, endpoint, 0, buf)?;

        transfer.wait().await?;

        if transfer.state.urb.actual_length as usize != buffer.len() {
            return Err(err_msg("Not all bytes were written"));
        }

        Ok(())
    }
}
