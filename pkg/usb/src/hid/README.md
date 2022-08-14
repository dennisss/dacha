
TODO: In our cluster os, disable all the HID drivers in the kernel


HID Device Primer

- Is a USB device with an HID interface
    - bInterfaceClass == InterfaceClass::HID
- An HID interface has an HIDDescriptor immediately after the interface descriptor
    - When the configuration descriptor is queried with GET_DESCRIPTOR, only the HIDDescriptor is included and not any report descriptors
- Has an Interrupt IN endpoint
- Optionally has an Interrupt OUT endpoint
    - If present, this is used for writing output reports instead of using SET_REPORT on the control interface.

- Basically we communicate with the device by reading input reports or writing output reports.
- Reports are little endian 



- bCountryCode can be  
    - 0 for not supported
    - 33 for US

    - bInterval will be important for polling

usages (defined in the separate HID Usages Table doc):
- Generic Desktop (0x01)
    - Keyboard (0x06)
- LED (0x08)
- Keyboard / KeyPad page (0x07)



Boot Keyboard requirements are in Appendix B

Also See Appendix G for request support
    Must Have GET_REPORT, GET_IDLE, SET_IDLE, GET_PROTOCOL, SET_PROTOCOL
    - Note: Protocol defaults to non-boot


So basicaly
