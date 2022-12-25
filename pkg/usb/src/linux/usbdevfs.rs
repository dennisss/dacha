// Bindings and constants used by the Linux USBDEVFS driver.
//
// These are mostly copied from:
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/usbdevice_fs.h
//
// See this page for URB status meanings:
// https://www.kernel.org/doc/html/v5.5/driver-api/usb/error-codes.html

pub const USBDEVFS_PATH: &'static str = "/dev/bus/usb";

const USBDEVFS_IOC_MAGIC: u8 = b'U';

pub const USBDEVFS_IOC_DISCONNECT: sys::c_int =
    request_code_none!(USBDEVFS_IOC_MAGIC, 22) as sys::c_int;
pub const USBDEVFS_IOC_CONNECT: sys::c_int =
    request_code_none!(USBDEVFS_IOC_MAGIC, 23) as sys::c_int;

pub const USBDEVFS_URB_SHORT_NOT_OK: sys::c_uint = 0x01;
pub const USBDEVFS_URB_ISO_ASAP: sys::c_uint = 0x02;
pub const USBDEVFS_URB_BULK_CONTINUATION: sys::c_uint = 0x04;
pub const USBDEVFS_URB_NO_FSBR: sys::c_uint = 0x20;

/// If set in the URB flags, when sending a transfer that is a multiple of the
/// max packet length, we will append a zero length packet to the transfer. This
/// only really matters for bulk out transfers.
pub const USBDEVFS_URB_ZERO_PACKET: sys::c_uint = 0x40;

pub const USBDEVFS_URB_NO_INTERRUPT: sys::c_uint = 0x80;

pub const USBDEVFS_URB_TYPE_ISO: u8 = 0;
pub const USBDEVFS_URB_TYPE_INTERRUPT: u8 = 1;
pub const USBDEVFS_URB_TYPE_CONTROL: u8 = 2;
pub const USBDEVFS_URB_TYPE_BULK: u8 = 3;

#[repr(C)]
pub struct usbdevfs_setinterface {
    pub interface: sys::c_uint,
    pub altsetting: sys::c_uint,
}

// TODO: This is only thread safe if we access it after the linux kernel reaps
// it.
#[derive(Debug)]
#[repr(C)]
pub struct usbdevfs_urb {
    pub typ: sys::c_uchar,
    pub endpoint: sys::c_uchar,
    pub status: sys::c_int,
    pub flags: sys::c_uint,
    pub buffer: sys::uintptr_t,
    pub buffer_length: sys::c_int,
    pub actual_length: sys::c_int,
    pub start_frame: sys::c_int,
    pub stream_id: sys::c_uint, // union with number_of_packets
    pub error_count: sys::c_int,
    pub signr: sys::c_uint,
    pub usrcontext: sys::uintptr_t,
}

#[repr(C)]
pub struct usbdevfs_getdriver {
    pub interface: sys::c_uint,
    pub driver: [sys::c_uchar; 256],
}

#[repr(C)]
pub struct usbdevfs_ioctl {
    pub ifno: sys::c_int,
    pub ioctl_code: sys::c_int,
    pub data: sys::uintptr_t,
}

ioctl_write_ptr!(
    usbdevfs_getdriver_fn,
    USBDEVFS_IOC_MAGIC,
    8,
    usbdevfs_getdriver
);
ioctl_read!(
    usbdevfs_setinterface_fn,
    USBDEVFS_IOC_MAGIC,
    4,
    usbdevfs_setinterface
);
ioctl_read!(
    usbdevfs_claim_interface,
    USBDEVFS_IOC_MAGIC,
    15,
    sys::c_uint
);
ioctl_read!(
    usbdevfs_release_interface,
    USBDEVFS_IOC_MAGIC,
    16,
    sys::c_uint
);
ioctl_read!(usbdevfs_submiturb, USBDEVFS_IOC_MAGIC, 10, usbdevfs_urb);
ioctl_write_ptr_bad!(
    usbdevfs_discardurb,
    request_code_none!(USBDEVFS_IOC_MAGIC, 11),
    usbdevfs_urb
);
ioctl_write_ptr!(usbdevfs_reapurb, USBDEVFS_IOC_MAGIC, 12, &usbdevfs_urb);
ioctl_write_ptr!(
    usbdevfs_reapurbndelay,
    USBDEVFS_IOC_MAGIC,
    13,
    *const usbdevfs_urb
);
ioctl_readwrite!(usbdevfs_ioctl_fn, USBDEVFS_IOC_MAGIC, 18, usbdevfs_ioctl);
ioctl_none!(usbdevfs_reset, USBDEVFS_IOC_MAGIC, 20);
ioctl_read!(
    usbdevfs_setconfiguration,
    USBDEVFS_IOC_MAGIC,
    5,
    sys::c_uint
);
