# Embedded Device Communication Protocols

The goal here is to standardize some mechanisms for communicating with embedded devices.


/*
Serial protocol:
- Byte 0: Length of bytes [3..)
- Byte 1-2: Little endian CRC-16 of bytes 3+
- Byte 3: Type of this packet (ProtocolRequestType)
- Byte 4+: Payload bytes specific to the request type.

Requests can also be received via the interrupt IN endpoint of the protocol's USB endpoint
- In this case, only packets 3+ of the serial protocol are sent.

TODO: Given that we only support 64-byte interrupt in packets, 


BoardDescriptor {
    fixed32 chip = 1;

    uint32 device_id = 2;

    uint32 hardware_revision_id = 3;
 
    bytes build_id = 4;
}
*/

USB standardization:
    - Vendor Id will always be 0x8888
    - Device Ids are allocated per meaningfully different things:
        - Hardware revisions are ignored in this and some cross multiple revisions

Device Ids {
    Bootloader = 1,
    DevBoard = 2,
    RadioDongle,
    RadioSerial,
    CNC,
    Keyboard,
    FanController,

}

Things that are important to track for hardware:
    -


Requirements:
- Must be able to query the build id of a device
    -


message SignedPacket {
    HandshakePacket packet = 1;
    bytes signature = 2 [max_length = ]; 
}

message HandshakePacket {
    oneof packet {
        NewSessionPacket new_session = 1;
        StartSessionPacket start_session = 2;
    }
}

message NewSessionPacket {
    bytes public_value_a = 1 [max_length = 32];
    uint32 preferred_encryption_slot = 2;
}

message StartSessionPacket {
    bytes public_value_a = 1 [max_length = 32];
    bytes public_value_b = 2 [max_length = 32];
    uint32 encryption_slot = 3;
}