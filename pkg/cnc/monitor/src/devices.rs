use base_error::*;
use cnc_monitor_proto::cnc::DeviceSelector;
use common::io::{Readable, Writeable};
use file::{LocalPath, LocalPathBuf};
use media_web::camera_manager::{CameraManager, CameraSubscriber};
use peripherals::serial::SerialPort;

use crate::fake_machine::FakeMachine;

#[derive(Clone)]
pub enum AvailableDevice {
    USB(AvailableUSBDevice),
    Fake(usize),
}

#[derive(Clone)]
pub struct AvailableUSBDevice {
    pub usb_entry: usb::DeviceEntry,
    pub device_descriptor: usb::descriptors::DeviceDescriptor,
    /// Serial number.
    pub serial_number: String,
    pub driver_devices: Vec<usb::DriverDevice>,

    pub vendor_name: Option<String>,
    pub product_name: Option<String>,
}

impl AvailableDevice {
    pub async fn list_all(usb_context: &usb::Context) -> Result<Vec<Self>> {
        let mut out = vec![];

        let devices = usb_context.enumerate_devices().await?;
        for device in devices {
            let device_descriptor = device.device_descriptor()?;
            let serial = device.serial().await?.unwrap_or(String::new());
            let driver_devices = device.driver_devices().await?;
            let vendor_name = device.manufacturer().await?;
            let product_name = device.product().await?;

            out.push(AvailableDevice::USB(AvailableUSBDevice {
                usb_entry: device,
                device_descriptor,
                serial_number: serial,
                driver_devices,
                vendor_name,
                product_name,
            }));
        }

        Ok(out)
    }

    /// Unique path/id for this device.
    ///
    /// Two devices with the same path are considered to be equivalent (this
    /// shouldn't be a need to re-connect to a device if metadata other than the
    /// path changes).
    ///
    /// NOTE: This is not long term stable. e.g. switching a USB device from one
    /// port to another will change its path.
    pub fn path(&self) -> LocalPathBuf {
        match self {
            Self::USB(dev) => dev.usb_entry.sysfs_dir().to_owned(),
            Self::Fake(i) => LocalPath::new(&format!("/fake/{}", *i)).to_owned(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::USB(dev) => {
                format!(
                    "USB Device {}:{}",
                    dev.usb_entry.bus_num(),
                    dev.usb_entry.dev_num()
                )
            }
            Self::Fake(i) => {
                format!("Fake #{}", i)
            }
        }
    }

    pub fn matches(&self, selector: &DeviceSelector) -> bool {
        if selector.has_usb() {
            let dev = match self {
                Self::USB(d) => d,
                _ => return false,
            };

            if selector.usb().vendor() as u16 != dev.device_descriptor.idVendor {
                return false;
            }

            if selector.usb().product() as u16 != dev.device_descriptor.idProduct {
                return false;
            }

            if !selector.usb().serial_number().is_empty()
                && selector.usb().serial_number() != dev.serial_number
            {
                return false;
            }
        }

        if selector.has_fake() {
            let i = match self {
                Self::Fake(i) => i,
                _ => return false,
            };

            if *i != selector.fake() as usize {
                return false;
            }
        }

        true
    }

    pub fn stable_selector(&self) -> DeviceSelector {
        let mut sel = DeviceSelector::default();

        match self {
            Self::USB(dev) => {
                sel.usb_mut()
                    .set_vendor(dev.device_descriptor.idVendor as u32);
                sel.usb_mut()
                    .set_product(dev.device_descriptor.idProduct as u32);
                sel.usb_mut().set_serial_number(dev.serial_number.clone());
            }
            Self::Fake(i) => {
                sel.set_fake(*i as u32);
            }
        }

        sel
    }

    pub fn verbose_proto(&self) -> DeviceSelector {
        let mut sel = DeviceSelector::default();

        sel.set_path(self.path().as_str());

        match self {
            Self::USB(dev) => {
                sel.usb_mut()
                    .set_vendor(dev.device_descriptor.idVendor as u32);
                sel.usb_mut()
                    .set_product(dev.device_descriptor.idProduct as u32);
                sel.usb_mut().set_serial_number(dev.serial_number.clone());

                if let Some(v) = &dev.vendor_name {
                    sel.usb_mut().set_vendor_name(v);
                }

                if let Some(v) = &dev.product_name {
                    sel.usb_mut().set_product_name(v);
                }

                for driver in &dev.driver_devices {
                    match driver.typ {
                        usb::DriverDeviceType::TTY => {
                            sel.add_serial_path(driver.path.as_str().into());
                        }
                        usb::DriverDeviceType::V4L2 => {
                            sel.add_video_path(driver.path.as_str().into());
                        }
                        _ => {}
                    }
                }
            }
            Self::Fake(i) => {
                sel.set_fake(*i as u32);
                sel.add_serial_path(format!("/fake/{}", *i));
            }
        };

        sel
    }

    pub async fn open_as_serial_port(
        &self,
        baud_rate: usize,
    ) -> Result<(Box<dyn Readable>, Box<dyn Writeable>)> {
        match self {
            Self::USB(device) => {
                let mut serial_path = None;
                let mut failed = false;
                for dev in &device.driver_devices {
                    if dev.typ == usb::DriverDeviceType::TTY {
                        if serial_path.is_some() {
                            return Err(err_msg("USB device exposes multiple serial ports"));
                        }

                        serial_path = Some(dev.path.clone());
                    }
                }

                let serial_path =
                    serial_path.ok_or_else(|| err_msg("No serial port exposed by USB device"))?;

                let serial = SerialPort::open(serial_path, baud_rate)?;
                let (serial_reader, serial_writer) = serial.split();

                Ok((serial_reader, serial_writer))
            }
            Self::Fake(i) => FakeMachine::create().await,
        }
    }

    pub async fn open_as_camera(&self, camera_manager: &CameraManager) -> Result<CameraSubscriber> {
        match self {
            Self::USB(device) => {
                camera_manager
                    .open_usb_camera(device.usb_entry.clone())
                    .await
            }
            _ => return Err(err_msg("Unsupported device type for camera")),
        }
    }
}
