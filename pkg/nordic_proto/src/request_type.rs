enum_def_with_unknown!(ProtocolRequestType u8 =>
    // Send a packet.
    // The payload is interprated as a PacketBuffer.
    //
    // Encryption of the packet is handled by the device.
    //
    // Either the device or the host can be set up to control the packet counter:
    // When the packet counter is 0 it will be
    // assigned by the device. If the device doesn't support persistent storage, this will fail.
    // If the packet counter is non-zero, it will be used when sending to the remote host. If the
    // device does have its own persistent storage set up, this will fail.
    //
    // [Host -> Device]
    Send = 1,

    // [Device -> Host]
    Receive = 2,

    // Reads the value of the last packet counter sent by this device.
    //
    // [Device -> Host]
    LastPacketCounter = 3,

    SetNetworkConfig = 4,

    // Retrieves the current NetworkConfig proto used by this device.
    // Will return empty data if no valid config is present.
    //
    // [Device -> Host]
    GetNetworkConfig = 5,

    // Reads the next entries from the device's internal log.
    //
    // - The host should allow for a buffer with at least 256 bytes.
    // - The format of the returned data is a concatenated list of ordered log entries where each
    //   entry is of the form:
    //   [length: u8] [data: LogEntry proto]
    //
    // [Device -> Host]
    ReadLog = 6
);
