enum_def_with_unknown!(ProtocolUSBRequestType u8 =>
    // [Host -> Device] Send a packet.
    // The payload is interprated as a PacketBuffer.
    //
    // Encryption of the packet is handled by the device.
    //
    // Either the device or the host can be set up to control the packet counter:
    // When the packet counter is 0 it will be
    // assigned by the device. If the device doesn't support persistent storage, this will fail.
    // If the packet counter is non-zero, it will be used when sending to the remote host. If the
    // device does have its own persistent storage set up, this will fail.
    Send = 1,

    // [Device -> Host]
    Receive = 2,

    // [Device -> Host] Reads the value of the last packet counter sent by this device.
    LastPacketCounter = 3,

    SetNetworkConfig = 4,

    GetNetworkConfig = 5
);
