use crate::packet::PacketBuffer;

/// Max size of a keyboard radio packet.
const KEYBOARD_PACKET_SIZE: usize = 28;

/// Must subtract the type byte and the session_id bytes.
const KEYBOARD_STATE_SIZE: usize = KEYBOARD_PACKET_SIZE - 1;

/*
Key functionalities we need:
- We should support making sure


- So dongle doesn't need any state?
- The dongle will always simply advance the counter to the one used by the keyboard so no dongle side syncronization is needed.
- Keyboard does need to maintain state as using old counters is still really bad.
- If counters never go backwards then I guess we never need to

- Replay on the dongle can force it to re-use old counters.
-


We don't need peripheral level

*/

pub enum KeyboardPacket {
    State { state: [u8; KEYBOARD_STATE_SIZE] },
    AcknowledgeState { state_counter: u32 },
}

enum_def_with_unknown!(KeyboardPacketType u8 =>
    State = 1,
    AcknowledgeState = 2
);

impl KeyboardPacket {
    pub fn parse_from(packet: &PacketBuffer) -> Option<Self> {
        let data = packet.data();

        if data.len() != KEYBOARD_PACKET_SIZE {
            return None;
        }

        let typ = KeyboardPacketType::from_value(data[0]);

        match typ {
            KeyboardPacketType::State => {
                let state = *array_ref![data, 1, KEYBOARD_STATE_SIZE];
                Some(Self::State { state })
            }
            KeyboardPacketType::AcknowledgeState => {
                let state_counter = u32::from_le_bytes(*array_ref![data, 1, 4]);
                Some(Self::AcknowledgeState { state_counter })
            }
            KeyboardPacketType::Unknown(_) => None,
        }
    }

    pub fn serialize_to(&self, packet: &mut PacketBuffer) {
        packet.resize_data(KEYBOARD_PACKET_SIZE);

        let data = packet.data_mut();
        for i in 0..data.len() {
            data[i] = 0;
        }

        match self {
            Self::State { state } => {
                data[0] = KeyboardPacketType::State.to_value();
                data[1..].copy_from_slice(state);
            }
            Self::AcknowledgeState { state_counter } => {
                data[0] = KeyboardPacketType::AcknowledgeState.to_value();
                *array_mut_ref![data, 1, 4] = state_counter.to_le_bytes();
            }
        }
    }
}
