use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use file::{FileError, LocalPath, LocalPathBuf};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::thread;

use common::{errors::*, futures::StreamExt};

use crate::descriptor_iter::{Descriptor, DescriptorIter};
use crate::descriptors::*;
use crate::linux::device::*;
use crate::linux::transfer::*;
use crate::linux::usbdevfs::*;

const SYSFS_PATH: &'static str = "/sys/bus/usb/devices";

/*
USB Dev FS:
    /dev/bus/usb/001/001

SYS FS
    /sys/bus/usb/devices

Possibly check /sys/class/tty/ for a connection to TTY devices.

*/

/// Shared state and manager for multiple open USB devices.
///
/// Generally you will only need to create one of these for a program using
/// Context::create() and then you can use it to open one or more devices.
///
/// Internally this runs a single background thread for polling all of the
/// devices created using the same context.
///
/// Drop semantics:
/// - The context will be cleaned up when all references to the context are
///   dropped. Meaning:
///   - All copies of the Arc<Context> returned by Context::create() is dropped.
///   - All Devices opened using the Context are closed (dropped).
/// - Implementation details
///   - User's will always be given an Arc<Context>
///   - Internally the Context and every open Device contains a
///     Arc<ContextState>
#[derive(Clone)]
pub struct Context {
    state: Arc<ContextState>,
}

pub(crate) struct ContextState {
    /// Reference to the background thread that receives context/device events.
    /// NOTE: This will always be not-None after Context::create() is done.
    background_thread_handle: std::sync::Mutex<Option<thread::JoinHandle<()>>>,

    /// File descriptor for the eventfd() used to notify the background thread
    /// when a change to the open devices has occured.
    background_thread_eventfd: sys::c_int,

    /// The sending end of the set of all channels which are waiting for the
    /// background thread to finish executing at least one cycle.
    background_thread_waiters: std::sync::Mutex<Vec<std::sync::mpsc::SyncSender<()>>>,

    /// All devices that were opened under this context.
    devices: std::sync::Mutex<ContextDevices>,
}

impl Drop for ContextState {
    fn drop(&mut self) {
        self.close();
    }
}

impl ContextState {
    // TODO: Don't make this public! When this is called, it won't destroy the
    // devices so must only be called after all devices are destroyed.
    fn close(&mut self) {
        // TODO: Check return value
        unsafe { sys::close(self.background_thread_eventfd) };

        // NOTE: The background thread should terminate itself shortly now that
        // the eventfd is dead.
    }
}

impl Context {
    pub fn create() -> Result<Self> {
        let background_thread_eventfd =
            unsafe { sys::eventfd2(0, sys::O_CLOEXEC | sys::O_NONBLOCK) }?;

        // TODO: Drop the outer Arc and return a regular object.
        let instance = Context {
            state: Arc::new(ContextState {
                background_thread_eventfd,
                background_thread_handle: std::sync::Mutex::new(None),
                background_thread_waiters: std::sync::Mutex::new(vec![]),
                devices: std::sync::Mutex::new(ContextDevices {
                    open_devices: HashMap::new(),
                    last_device_id: 0,
                }),
            }),
        };

        let background_state = instance.state.clone();
        *instance.state.background_thread_handle.lock().unwrap() = Some(thread::spawn(move || {
            Self::run_background_thread(background_state)
        }));

        Ok(instance)
    }

    fn run_background_thread(context_state: Arc<ContextState>) {
        let mut fds = vec![];
        loop {
            {
                let mut waiters = context_state.background_thread_waiters.lock().unwrap();
                for waiter in waiters.iter() {
                    let _ = waiter.send(());
                }
                waiters.clear();
            }

            {
                fds.clear();

                fds.push(sys::pollfd {
                    fd: context_state.background_thread_eventfd,
                    events: sys::POLLIN,
                    revents: 0,
                });

                let devices = context_state.devices.lock().unwrap();
                for (_, dev) in devices.open_devices.iter() {
                    if *dev.has_error.lock().unwrap() {
                        continue;
                    }

                    fds.push(sys::pollfd {
                        fd: dev.fd,
                        events: sys::POLLOUT,
                        revents: 0,
                    });
                }
            }

            let poll_result =
                unsafe { sys::poll(fds.as_mut_ptr(), fds.len() as sys::c_uint, 1000) };

            let n = match poll_result {
                Ok(0) => {
                    // Timeout
                    continue;
                }
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Polling error: {:?}", e);
                    return;
                }
            };

            for fd in &fds {
                if fd.revents == 0 {
                    continue;
                }

                // This means that
                // TODO: Also check for hangups
                if fd.fd == context_state.background_thread_eventfd {
                    if (fd.revents & sys::POLLNVAL) != 0 {
                        // Context was closed. No point in continuing to poll.
                        return;
                    }

                    // Read the fd to clear the value so that it doesn't continue receiving events.
                    let mut event_num: u64 = 0;
                    let n = unsafe {
                        sys::read(
                            fd.fd,
                            core::mem::transmute(&mut event_num),
                            core::mem::size_of::<u64>(),
                        )
                    };

                    if n != Ok(core::mem::size_of::<u64>()) {
                        println!("Failed to read eventfd!!");
                    }

                    // TODO: Verify event_num is non-zero.

                    continue;
                }

                if (fd.revents & sys::POLLNVAL) != 0 {
                    // The fd was closed. Most likely this means that the device was just closed.
                    // Next time we poll() the fd should no longer be in our devices list.
                    continue;
                } else if (fd.revents & (sys::POLLERR | sys::POLLHUP)) != 0 {
                    // Usually this will happen when the USB device is disconnected externally.
                    // We'll make that the device has an error so that we don't poll it anymore.
                    // We assume that after this point, future syscalls on this file will continue
                    // to return errors.

                    let mut devices = context_state.devices.lock().unwrap();
                    for (_, device) in devices.open_devices.iter_mut() {
                        if device.fd == fd.fd {
                            *device.has_error.lock().unwrap() = true;
                            break;
                        }
                    }
                } else if (fd.revents & sys::POLLOUT) != 0 {
                    // TODO: Ensure that the receiver handles any status in the URB.

                    let ptr: *const usbdevfs_urb = core::ptr::null();
                    for _ in 0..10 {
                        let res = unsafe { usbdevfs_reapurbndelay(fd.fd, &ptr) };
                        match res {
                            Ok(_) => {}
                            Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)) => {
                                // There are no completed URBs ready to reap without blocking.
                                break;
                            }
                            Err(nix::Error::Sys(nix::errno::Errno::EBADF)) => {
                                // The device was just closed by the user. On the next poll cycle,
                                // we shouldn't poll this file anymore.
                                break;
                            }
                            // TODO: Figure out what the error code will be after the device is
                            // closed.
                            Err(e) => {
                                panic!("{}", e);
                            }
                        }

                        let urb: &usbdevfs_urb = unsafe { &*ptr };
                        let transfer: &DeviceTransferState =
                            unsafe { core::mem::transmute(urb.usrcontext) };

                        transfer.perform_reap();

                        // NOTE: We can't read any the transfer memory from now on as it may have
                        // been dropped in the previous line.
                        drop(transfer);
                    }
                } else {
                    eprintln!("Unhandled poll event!");
                }
            }
        }
    }

    pub async fn open_device(&self, vendor_id: u16, product_id: u16) -> Result<Device> {
        let mut device = None;
        for device_entry in self.enumerate_devices().await? {
            let device_desc = device_entry.device_descriptor()?;
            if device_desc.idVendor == vendor_id && device_desc.idProduct == product_id {
                device = Some(device_entry.open().await?);
            }
        }

        device.ok_or_else(|| err_msg("No device found"))
    }

    /// Lists all USB devices attached to the computer.
    ///
    /// Internally this uses sysfs for similar reasons to libusb. In particular
    /// this enables us to use cached kernel device descriptors rather than
    /// opening each device.
    pub async fn enumerate_devices(&self) -> Result<Vec<DeviceEntry>> {
        let mut out = vec![];

        for entry in file::read_dir(SYSFS_PATH)? {
            let path = LocalPath::new(SYSFS_PATH).join(entry.name());
            let file_name = path.file_name().unwrap_or_default();
            // let file_type = entry.file_type().await?;

            // Only a "7-3.4" ones.
            if file_name.starts_with("usb") || file_name.contains(":") {
                continue;
            }

            out.push(self.enumerate_single_device(&path).await?);
        }

        Ok(out)
    }

    async fn enumerate_single_device(&self, sysfs_dir: &LocalPathBuf) -> Result<DeviceEntry> {
        let busnum = file::read_to_string(sysfs_dir.join("busnum"))
            .await?
            .trim_end()
            .parse::<usize>()?;

        let devnum = file::read_to_string(sysfs_dir.join("devnum"))
            .await?
            .trim_end()
            .parse::<usize>()?;

        let raw_descriptors = file::read(sysfs_dir.join("descriptors")).await?;

        Ok(DeviceEntry {
            context_state: self.state.clone(),
            busnum,
            devnum,
            raw_descriptors,

            sysfs_dir: sysfs_dir.to_owned(),
            usbdevfs_path: LocalPath::new(USBDEVFS_PATH)
                .join(format!("{:03}/{:03}", busnum, devnum)),
        })
    }
}

impl ContextState {
    pub(crate) fn add_background_thread_waiter(&self) -> std::sync::mpsc::Receiver<()> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let mut waiters = self.background_thread_waiters.lock().unwrap();
        waiters.push(sender);
        receiver
    }

    pub(crate) fn notify_background_thread(&self) -> Result<()> {
        // TODO: If this fails, should we remove the device from the list?
        let event_num: u64 = 1;
        let n = unsafe {
            sys::write(
                self.background_thread_eventfd,
                core::mem::transmute(&event_num),
                core::mem::size_of::<u64>(),
            )
        };
        if n != Ok(core::mem::size_of::<u64>()) {
            return Err(err_msg("Failed to notify background thread"));
        }

        // TODO: Ignore EAGAIN errors. Mains that the counter overflowed (meaning that
        // it already has a value set.)

        Ok(())
    }

    pub(crate) fn add_device(&self, state: Arc<DeviceState>) -> Result<usize> {
        let mut devices = self.devices.lock().unwrap();

        for (_, device_state) in devices.open_devices.iter() {
            if device_state.bus_num == state.bus_num && device_state.dev_num == state.dev_num {
                return Err(err_msg("Device already opened under this context"));
            }
        }

        let id = devices.last_device_id + 1;
        devices.last_device_id = id;
        devices.open_devices.insert(id, state);
        drop(devices);

        self.notify_background_thread()?;

        Ok(id)
    }

    pub(crate) fn remove_device(&self, id: usize) -> Result<()> {
        let mut devices = self.devices.lock().unwrap();
        devices.open_devices.remove(&id);
        drop(devices);

        self.notify_background_thread()?;
        Ok(())
    }
}

struct ContextDevices {
    open_devices: HashMap<usize, Arc<DeviceState>>,
    last_device_id: usize,
}

/// Reference to a device connected to the system.
/// Can be used to open the device or preview descriptors that are cached by the
/// system.
pub struct DeviceEntry {
    context_state: Arc<ContextState>,
    busnum: usize,
    devnum: usize,
    raw_descriptors: Vec<u8>,
    sysfs_dir: LocalPathBuf,
    usbdevfs_path: LocalPathBuf,
}

impl DeviceEntry {
    pub fn device_descriptor(&self) -> Result<DeviceDescriptor> {
        match self.descriptors().next() {
            Some(Ok(Descriptor::Device(d))) => Ok(d),
            _ => Err(err_msg(
                "Expected first cached descriptor to be a device descriptor",
            )),
        }
    }

    pub fn descriptors<'a>(&'a self) -> impl Iterator<Item = Result<Descriptor>> + 'a {
        DescriptorIter::new(&self.raw_descriptors)
    }

    // NOTE: This is mainly exposed for the purpose of mounting into containers.
    pub fn sysfs_dir(&self) -> &LocalPath {
        &self.sysfs_dir
    }

    // NOTE: This is mainly exposed for the purpose of mounting into containers.
    pub fn devfs_path(&self) -> &LocalPath {
        &self.usbdevfs_path
    }

    pub fn bus_num(&self) -> usize {
        self.busnum
    }

    pub fn dev_num(&self) -> usize {
        self.devnum
    }

    async fn get_sysfs_value(&self, key: &str) -> Result<Option<String>> {
        match file::read_to_string(self.sysfs_dir.join(key)).await {
            Ok(s) => Ok(Some(s.trim_end().to_string())),
            Err(e) => {
                if let Some(FileError::NotFound) = e.downcast_ref() {
                    return Ok(None);
                }

                Err(e.into())
            }
        }
    }

    pub async fn manufacturer(&self) -> Result<Option<String>> {
        self.get_sysfs_value("manufacturer").await
    }

    pub async fn product(&self) -> Result<Option<String>> {
        self.get_sysfs_value("product").await
    }

    pub async fn serial(&self) -> Result<Option<String>> {
        self.get_sysfs_value("serial").await
    }

    pub async fn open(&self) -> Result<Device> {
        let path = CString::new(self.usbdevfs_path.as_str())?;
        let fd = match unsafe { sys::open(path.as_ptr(), sys::O_RDWR | sys::O_CLOEXEC, 0) } {
            Ok(v) => v,
            Err(e) => {
                return Err(format_err!("Failed to open USB device: {:?}", e));
            }
        };

        let state = Arc::new(DeviceState {
            bus_num: self.busnum,
            dev_num: self.devnum,
            has_error: std::sync::Mutex::new(false),
            fd,
            fd_closed: std::sync::Mutex::new(false),
            transfers: std::sync::Mutex::new(DeviceStateTransfers::default()),
        });

        // TODO: If this fails, then we need to remove the device from the list.
        Device::create(self.context_state.clone(), state, &self.raw_descriptors)
    }
}
