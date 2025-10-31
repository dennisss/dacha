use std::collections::HashMap;
use alloc::string::String;
use alloc::string::ToString;

use common::errors::*;

use crate::udp::MessageSocket;

const UDEV_MAGIC: &'static [u8] = b"libudev\x00\xFE\xED\xCA\xFE";

/// Socket that receives device events from the system wide udev process.
///
/// Note that this requires being in the main network namespace. This is different
/// from the raw uevent events sent from the kernel.
///
/// Kernel events look like:
/// remove@/devices/pci0000:00/0000:00:08.1/0000:0c:00.3/usb7/7-2/7-2.2/7-2.2.1/7-2.2.1.1/7-2.2.1.1.3/7-2.2.1.1.3:1.0/ttyUSB0/tty/ttyUSB0\x00ACTION=remove\x00DEVPATH=/devices/pci0000:00/0000:00:08.1/0000:0c:00.3/usb7/7-2/7-2.2/7-2.2.1/7-2.2.1.1/7-2.2.1.1.3/7-2.2.1.1.3:1.0/ttyUSB0/tty/ttyUSB0\x00SUBSYSTEM=tty\x00MAJOR=188\x00MINOR=0\x00DEVNAME=ttyUSB0\x00SEQNUM=10482\x00
///
/// UDev events looks like:
/// libudev\x00\xFE\xED\xCA\xFE(\x00\x00\x00(\x00\x00\x00o\x03\x00\x00\x05w\xC5\xE5'\xF8\xF5\x0C\x02\x08 \x08\x00@\x10\tACTION=add\x00DEVPATH=/devices/pci0000:00/0000:00:08.1/0000:0c:00.3/usb7/7-2/7-2.2/7-2.2.1/7-2.2.1.1/7-2.2.1.1.3\x00SUBSYSTEM=usb\x00DEVNAME=/dev/bus/usb/007/056\x00DEVTYPE=usb_device\x00PRODUCT=403/6001/600\x00TYPE=0/0/0\x00BUSNUM=007\x00DEVNUM=056\x00SEQNUM=10492\x00MAJOR=189\x00MINOR=823\x00USEC_INITIALIZED=160487356454\x00ID_VENDOR=FTDI\x00ID_VENDOR_ENC=FTDI\x00ID_VENDOR_ID=0403\x00ID_MODEL=FT232R_USB_UART\x00ID_MODEL_ENC=FT232R\x20USB\x20UART\x00ID_MODEL_ID=6001\x00ID_REVISION=0600\x00ID_SERIAL=FTDI_FT232R_USB_UART_AR0KMKX4\x00ID_SERIAL_SHORT=AR0KMKX4\x00ID_BUS=usb\x00ID_USB_INTERFACES=:ffffff:\x00ID_VENDOR_FROM_DATABASE=Future Technology Devices International, Ltd\x00ID_MODEL_FROM_DATABASE=FT232 Serial (UART) IC\x00ID_PATH=pci-0000:0c:00.3-usb-0:2.2.1.1.3\x00ID_PATH_TAG=pci-0000_0c_00_3-usb-0_2_2_1_1_3\x00TAGS=:seat:uaccess:\x00CURRENT_TAGS=:seat:uaccess:\x00DRIVER=usb\x00ID_MM_DEVICE_MANUAL_SCAN_ONLY=1\x00ID_FOR_SEAT=usb-pci-0000_0c_00_3-usb-0_2_2_1_1_3\x00
///
/// Reference code:
/// https://github.com/systemd/systemd/blob/main/src/libsystemd/sd-device/device-monitor.c
pub struct UdevSocket {
    inner: MessageSocket
}

impl UdevSocket {
    pub fn create() -> Result<Self> {
        let fd = unsafe {
            sys::socket(
                sys::AddressFamily::AF_NETLINK,
                sys::SocketType::SOCK_RAW,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::NETLINK_KOBJECT_UEVENT,
            )?
        };

        /*
        group_id 1 is kernel events
        group_id 2 is libudev events
        */
        unsafe { sys::bind(&fd, &sys::SocketAddr::netlink(0, 2))? };

        Ok(Self { inner: MessageSocket::new(fd) })
    }

    pub async fn recv(&self) -> Result<UdevEvent> {
        let mut buf = [0u8; 8192];
        let n = self.inner.recv(&mut buf).await?;
        UdevEvent::parse_from(&buf[..n])
    }
}

#[derive(Debug, Clone)]
pub struct UdevEvent {
    pub properties: HashMap<String, String>
}

impl UdevEvent {
    pub fn parse_from(data: &[u8]) -> Result<Self> {
        if data.len() < UDEV_MAGIC.len() + 4 || &data[0..UDEV_MAGIC.len()] != UDEV_MAGIC {
            return Err(err_msg("Bad udev packet format"));
        }

        let header_size = u32::from_ne_bytes(*array_ref![data, UDEV_MAGIC.len(), 4]) as usize;
        if data.len() < header_size {
            return Err(err_msg("Bad udev message size"));
        }

        // TODO: Parse other stuff in the header before the properties.

        let mut properties = HashMap::default();

        let mut rest = &data[header_size..];
        if rest.len() > 0 && rest[rest.len() - 1] != 0x00 {
            return Err(err_msg("Last udev property doesn't end in a null byte"));
        }
        rest = &rest[..(rest.len() - 1)];

        for pair in rest.split(|v| *v == 0x00) {
            let s = std::str::from_utf8(pair)?;
            let (k, v) = s.split_once("=").ok_or_else(|| format_err!("Invalid key/value pair: {}", s))?;
            properties.insert(k.to_string(), v.to_string());
        }

        Ok(Self { properties })
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message() -> Result<()> {
        let message = b"libudev\x00\xFE\xED\xCA\xFE(\x00\x00\x00(\x00\x00\x00\x97\x02\x00\x00\x05w\xC5\xE5'\xF8\xF5\x0C\x02\x08\x00\x00\x00@\x00\x01ACTION=add\x00DEVPATH=/devices/pci0000:00/0000:00:08.1/0000:0c:00.3/usb7/7-2/7-2.2/7-2.2.1/7-2.2.1.4\x00SUBSYSTEM=usb\x00DEVNAME=/dev/bus/usb/007/015\x00DEVTYPE=usb_device\x00PRODUCT=8888/4/100\x00TYPE=0/0/0\x00BUSNUM=007\x00DEVNUM=015\x00SEQNUM=6787\x00MAJOR=189\x00MINOR=782\x00USEC_INITIALIZED=508363091924\x00ID_VENDOR=da_\x00ID_VENDOR_ENC=da\x21\x00ID_VENDOR_ID=8888\x00ID_MODEL=radio_dongle\x00ID_MODEL_ENC=radio\x20dongle\x00ID_MODEL_ID=0004\x00ID_REVISION=0100\x00ID_SERIAL=da__radio_dongle\x00ID_BUS=usb\x00ID_USB_INTERFACES=:fe0101:\x00ID_PATH=pci-0000:0c:00.3-usb-0:2.2.1.4\x00ID_PATH_TAG=pci-0000_0c_00_3-usb-0_2_2_1_4\x00DRIVER=usb\x00TAGS=:seat:\x00CURRENT_TAGS=:seat:\x00ID_FOR_SEAT=usb-pci-0000_0c_00_3-usb-0_2_2_1_4\x00.LOCAL_ifNum=\x00";

        println!("{:#?}", UdevEvent::parse_from(message)?);
        Ok(())
    }
}