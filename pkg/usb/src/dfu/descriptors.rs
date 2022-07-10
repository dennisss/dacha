use common::errors::*;

define_bit_flags!(
    DFUAttributes u8 {
        bitWillDetach = 1 << 3,
        bitManifestationTolerant = 1 << 2,
        bitCanUpload = 1 << 1,
        bitCanDnload = 1 << 0
    }
);

/// Value of SetupPacket::bRequest
pub enum DFURequestType {
    DFU_DETACH = 0,
    DFU_DNLOAD = 1,
    DFU_UPLOAD = 2,
    DFU_GETSTATUS = 3,
    DFU_CLRSTATUS = 4,
    DFU_GETSTATE = 5,
    DFU_ABORT = 6,
}

#[repr(packed)]
#[derive(Clone, Copy)]
pub struct DFUFunctionalDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bmAttributes: DFUAttributes,
    pub wDetachTimeOut: u16, // TODO: Is this big or little endian.
    pub wTransferSize: u16,
    pub bcdDFUVersion: u16,
}

pub const DFU_FUNCTIONAL_DESCRIPTOR_TYPE: u8 = 0x21;

/// Value of the bInterfaceSubClass field for an interface implementing a DFU
/// protocol. The corresponding InterfaceClass should be ApplicationSpecific.
pub const DFU_INTERFACE_SUBCLASS: u8 = 1;

enum_def_with_unknown!(DFUInterfaceProtocol u8 =>
    Runtime = 1,
    DFUMode = 2
);

#[repr(packed)]
pub struct DFUStatus {
    /// An indication of the status resulting from the execution of the most
    /// recent request.
    pub bStatus: DFUStatusCode,

    /// Minimum time, in milliseconds, that the host should wait before sending
    /// a subsequent DFU_GETSTATUS request.
    pub bwPollTimeout: [u8; 3],

    /// An indication of the state that the device is going to enter immediately
    /// following transmission of this response. (By the time the host receives
    /// this information, this is the current state of the device.)
    pub bState: DFUState,

    /// Index of status description in string table
    pub iString: u8,
}

macro_rules! define_enum {
    ($struct:ident, $($name:ident ( $code:expr ) => $message:expr),*) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $struct {
            code: u8,
        }

        impl $struct {
            $(
                pub const $name: Self = Self::from_raw($code);
            )*

            pub const fn from_raw(code: u8) -> Self {
                Self { code }
            }

            pub fn name(&self) -> Option<&'static str> {
                match self.code {
                    $(
                        $code => Some(stringify!($name)),
                    )*
                    _ => None
                }
            }

            pub fn default_description(&self) -> Option<&'static str> {
                match self.code {
                    $(
                        $code => Some($message),
                    )*
                    _ => None
                }
            }
        }
    };
}

define_enum!(
    DFUStatusCode,
    OK(0x00) => "",
    errTARGET(0x01) => "File is not targeted for use by this device.",
    errFILE(0x02) => "File is for this device but fails some vendor-specific verification test.",
    errWRITE(0x03) => "Device is unable to write memory.",
    errERASE(0x04) => "Memory erase function failed.",
    errCHECK_ERASED(0x05) => "Memory erase check failed.",
    errPROG(0x06) => "Program memory function failed.",
    errVERIFY(0x07) => "Programmed memory failed verification.",
    errADDRESS(0x08) => "Cannot program memory due to received address that is out of range.",
    errNOTDONE(0x09) => "Received DFU_DNLOAD with wLength = 0, but device does not think it has all of the data yet.",
    errFIRMWARE(0x0A) => "Device's firmware is corrupt. It cannot return to run-time (non-DFU) operations.",
    errVENDOR(0x0B) => "iString indicates a vendor-specific error.",
    errUSBR(0x0C) => "Device detected unexpected USB reset signaling.",
    errPOR(0x0D) => "Device detected unexpected power on reset.",
    errUNKNOWN(0x0E) => "Something went wrong, but the device does not know what it was.",
    errSTALLEDPKT(0x0F) => "Device stalled an unexpected request."
);

define_enum!(
    DFUState,
    appIDLE(0) => "Device is running its normal application.",
    appDETACH(1) => "Device is running its normal application, has received the DFU_DETACH request, and is waiting for a USB reset.",
    dfuIDLE(2) => "Device is operating in the DFU mode and is waiting for requests.",
    dfuDNLOAD_SYNC(3) => "Device has received a block and is waiting for the host to solicit the status via DFU_GETSTATUS.",
    dfuDNBUSY(4) => "Device is programming a control-write block into its nonvolatile memories.",
    dfuDNLOAD_IDLE(5) => "Device is processing a download operation. Expecting DFU_DNLOAD requests.",
    dfuMANIFEST_SYNC(6) => "Device has received the final block of firmware from the host and is waiting for receipt of DFU_GETSTATUS to begin the Manifestation phase; or device has completed the Manifestation phase and is waiting for receipt of
    DFU_GETSTATUS (Devices that can enter this state after the Manifestation phase set bmAttributes bit bitManifestationTolerant to 1.)",
    dfuMANIFEST(7) => "Device is in the Manifestation phase. (Not all devices will be able to respond to DFU_GETSTATUS when in this state.)",
    dfuMANIFEST_WAIT_RESET(8) => "Device has programmed its memories and is waiting for a USB reset or a power on reset. (Devices that must enter this state clear bitManifestationTolerant to 0.)",
    dfuUPLOAD_IDLE(9) => "The device is processing an upload operation. Expecting DFU_UPLOAD requests.",
    dfuERROR(10) => "An error has occurred. Awaiting the DFU_CLRSTATUS request."
);
