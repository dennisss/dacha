use crate::descriptor_builders::*;
use crate::descriptors::*;
use crate::dfu::descriptors::*;

// See USB DFU v1.1 spec, table 4.2
pub const DEFAULT_FUNCTIONAL_DESCRIPTOR: DFUFunctionalDescriptor = DFUFunctionalDescriptor {
    // TODO: Support automatically setting this for custom types.
    bLength: core::mem::size_of::<DFUFunctionalDescriptor>() as u8,
    bDescriptorType: DFU_FUNCTIONAL_DESCRIPTOR_TYPE,
    /*
    bitWillDetach: yes. device will reset itself on DFU_DETACH
    bitManifestationTolerant: no. device will reset itself during manifestation.
    bitCanUpload: yes
    bitCanDnload: yes
    */
    bmAttributes: DFUAttributes::bitWillDetach
        .or(DFUAttributes::bitManifestationTolerant)
        .or(DFUAttributes::bitCanUpload)
        .or(DFUAttributes::bitCanDnload),
    wDetachTimeOut: 1000,

    // Set to the size of 1 UF2 block.
    wTransferSize: 512,    // TODO: This can be > bMaxPacketSize0
    bcdDFUVersion: 0x0110, // v1.1
};

impl<'a> ConfigDescriptorSetBuilder<'a> {
    /// Adds descriptors that identify a device which can be rebooted into a DFU
    /// bootloader.
    ///
    /// TODO: How do we ensure that this always aligns with the bootloader's
    /// capabilities.
    pub fn add_dfu_runtime_interface(&mut self) -> &mut Self {
        // See USB DFU v1.1 spec, table 4.1
        self.add_interface(
            "::usb::dfu::DFUInterfaceNumberTag",
            InterfaceDescriptor {
                bLength: 0,
                bDescriptorType: 0,
                bInterfaceNumber: 1,
                bAlternateSetting: 0,
                bNumEndpoints: 0,
                bInterfaceClass: InterfaceClass::ApplicationSpecific.to_value(),
                bInterfaceSubClass: DFU_INTERFACE_SUBCLASS,
                bInterfaceProtocol: DFUInterfaceProtocol::Runtime.to_value(),
                iInterface: 0,
            },
        )
        .add_generic_descriptor(DEFAULT_FUNCTIONAL_DESCRIPTOR);

        self
    }

    /// Adds descriptors
    ///
    /// When calling this, the bDeviceClass and bDeviceSubClass should be 0 in
    /// the DeviceDescriptor.
    pub fn add_dfu_host_interface(&mut self) -> &mut Self {
        self.add_interface(
            "::usb::dfu::DFUInterfaceNumberTag",
            InterfaceDescriptor {
                bLength: 0,
                bDescriptorType: 0,
                bInterfaceNumber: 1,
                bAlternateSetting: 0,
                bNumEndpoints: 0,
                bInterfaceClass: InterfaceClass::ApplicationSpecific.to_value(),
                bInterfaceSubClass: DFU_INTERFACE_SUBCLASS,
                bInterfaceProtocol: DFUInterfaceProtocol::DFUMode.to_value(),
                iInterface: 0,
            },
        )
        .add_generic_descriptor(DEFAULT_FUNCTIONAL_DESCRIPTOR);

        self
    }
}
