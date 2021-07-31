// Bindings and constants used by the Linux USBDEVFS driver.
//
// These are mostly copied from:
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/usbdevice_fs.h
//
// See this page for URB status meanings:
// https://www.kernel.org/doc/html/v5.5/driver-api/usb/error-codes.html

pub const USBDEVFS_PATH: &'static str = "/dev/bus/usb";

const USBDEVFS_IOC_MAGIC: u8 = b'U';

pub const USBDEVFS_IOC_DISCONNECT: libc::c_int =
    request_code_none!(USBDEVFS_IOC_MAGIC, 22) as libc::c_int;
pub const USBDEVFS_IOC_CONNECT: libc::c_int =
    request_code_none!(USBDEVFS_IOC_MAGIC, 23) as libc::c_int;

pub const USBDEVFS_URB_SHORT_NOT_OK: libc::c_uint = 0x01;
pub const USBDEVFS_URB_ISO_ASAP: libc::c_uint = 0x02;
pub const USBDEVFS_URB_BULK_CONTINUATION: libc::c_uint = 0x04;
pub const USBDEVFS_URB_NO_FSBR: libc::c_uint = 0x20;
pub const USBDEVFS_URB_ZERO_PACKET: libc::c_uint = 0x40;
pub const USBDEVFS_URB_NO_INTERRUPT: libc::c_uint = 0x80;

pub const USBDEVFS_URB_TYPE_ISO: u8 = 0;
pub const USBDEVFS_URB_TYPE_INTERRUPT: u8 = 1;
pub const USBDEVFS_URB_TYPE_CONTROL: u8 = 2;
pub const USBDEVFS_URB_TYPE_BULK: u8 = 3;

#[repr(C)]
pub struct usbdevfs_setinterface {
    pub interface: libc::c_uint,
    pub altsetting: libc::c_uint,
}

// TODO: This is only thread safe if we access it after the linux kernel reaps
// it.
#[derive(Debug)]
#[repr(C)]
pub struct usbdevfs_urb {
    pub typ: libc::c_uchar,
    pub endpoint: libc::c_uchar,
    pub status: libc::c_int,
    pub flags: libc::c_uint,
    pub buffer: u64, // *const (),
    pub buffer_length: libc::c_int,
    pub actual_length: libc::c_int,
    pub start_frame: libc::c_int,
    pub stream_id: libc::c_uint, // union with number_of_packets
    pub error_count: libc::c_int,
    pub signr: libc::c_uint,

    pub usrcontext: u64, // *const ()
}

#[repr(C)]
pub struct usbdevfs_getdriver {
    pub interface: libc::c_uint,
    pub driver: [libc::c_uchar; 256],
}

#[repr(C)]
pub struct usbdevfs_ioctl {
    pub ifno: libc::c_int,
    pub ioctl_code: libc::c_int,
    pub data: u64, // *const ()
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
    libc::c_uint
);
ioctl_read!(
    usbdevfs_release_interface,
    USBDEVFS_IOC_MAGIC,
    16,
    libc::c_uint
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
    libc::c_uint
);
