enum_def_with_unknown!(ProtocolUSBRequestType u8 =>
    // [Host -> Device] Send a packet.
    Send = 1,
    // [Device -> Host]
    Receive = 2,
    // [Device -> Host] Reads the value of the last packet counter sent by this device.
    LastPacketCounter = 3,

    SetNetworkConfig = 4,

    GetNetworkConfig = 5
);
