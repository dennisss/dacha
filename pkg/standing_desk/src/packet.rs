use common::errors::*;

const TARGET_ADDRESS_OFFSET: usize = 0;
const COMMAND_OFFSET: usize = 2;
const PAYLOAD_LENGTH_OFFSET: usize = 3;
const PAYLOAD_OFFSET: usize = 4;

// 2 address + 1 command + 1 payload length + 1 checksum + 1 end byte
const NUM_NON_PAYLOAD_BYTES: usize = 6;

const MAX_PAYLOAD_LENGTH: usize = 3;

const END_BYTE: u8 = 0x7E;

pub enum Address {
    DeskControlBoard = 0xF1,
    ExternalDongle = 0xF2,
}

/// Command sent from the ExternalDongle to the DeskControlBoard.
pub enum RequestCommand {
    MoveUp = 0x01,
    MoveDown = 0x02,
    SetKey1 = 0x03,
    SetKey2 = 0x04,
    PressKey1 = 0x05,
    PressKey2 = 0x06,
    QueryState = 0x07,
    SetKey3 = 0x25,
    SetKey4 = 0x26,
}

pub enum ResponseCommand {
    /// Sent periodically whenever the desk moves and when QueryState is
    /// requested.
    CurrentHeight = 0x01,
    /// Sent when QueryState is requested.
    Key1Position = 0x25,
    /// Sent when QueryState is requested.
    Key2Position = 0x26,
    /// Sent when QueryState is requested.
    Key3Position = 0x27,
    /// Sent when QueryState is requested.
    Key4Position = 0x28,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Packet {
    pub target_address: u8,
    pub command: u8,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8])> {
        if input.len() < 4 {
            return Err(err_msg("Input too short"));
        }

        if input[0] != input[1] {
            return Err(err_msg("Mismatching address duplicate bytes"));
        }

        let target_address = input[TARGET_ADDRESS_OFFSET];
        let command = input[COMMAND_OFFSET];
        let payload_len = input[PAYLOAD_LENGTH_OFFSET] as usize;

        if payload_len > MAX_PAYLOAD_LENGTH || NUM_NON_PAYLOAD_BYTES + payload_len > input.len() {
            return Err(err_msg("Invalid payload length"));
        }

        let payload = &input[PAYLOAD_OFFSET..(PAYLOAD_OFFSET + payload_len)];
        let checksum = input[PAYLOAD_OFFSET + payload_len];
        let end_byte = input[PAYLOAD_OFFSET + payload_len + 1];

        if end_byte != END_BYTE {
            return Err(err_msg("Incorrect end byte"));
        }

        let expected_checksum =
            Self::calculate_checksum(&input[COMMAND_OFFSET..(PAYLOAD_OFFSET + payload_len)]);
        if expected_checksum != checksum {
            return Err(err_msg("Invalid checksum"));
        }

        Ok((
            Self {
                target_address,
                command,
                payload: payload.to_vec(),
            },
            &input[(NUM_NON_PAYLOAD_BYTES + payload_len)..],
        ))
    }

    pub fn parse_stream(mut input: &[u8]) -> Vec<Self> {
        let mut out = vec![];

        while !input.is_empty() {
            if let Ok((packet, rest)) = Self::parse(input) {
                out.push(packet);
                input = rest;
            } else {
                input = &input[1..];
            }
        }

        out
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![];
        out.reserve_exact(NUM_NON_PAYLOAD_BYTES + self.payload.len());
        out.extend_from_slice(&[
            self.target_address,
            self.target_address,
            self.command,
            self.payload.len() as u8,
        ]);
        out.extend_from_slice(&self.payload);

        let checksum = Self::calculate_checksum(&out[2..]);

        out.push(checksum);
        out.push(END_BYTE);

        out
    }

    fn calculate_checksum(data: &[u8]) -> u8 {
        let mut checksum: u8 = 0;
        for i in 0..data.len() {
            checksum = ((checksum as usize) + (data[i] as usize)) as u8;
        }

        checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_parse_successful() {
        let input = &[0xF2, 0xF2, 0x01, 0x03, 0x01, 56, 0x07, 68, 0x7E];
        assert_eq!(
            Packet::parse(input).unwrap(),
            (
                Packet {
                    target_address: 0xF2,
                    command: 0x01,
                    payload: vec![0x01, 56, 0x07]
                },
                &[] as &[u8]
            )
        );

        let input = &[0xF1, 0xF1, 0x04, 0x00, 0x04, 0x7E];
        assert_eq!(
            Packet::parse(input).unwrap(),
            (
                Packet {
                    target_address: 0xF1,
                    command: 0x04,
                    payload: vec![]
                },
                &[] as &[u8]
            )
        );
    }

    #[test]
    fn packet_parse_extra_bytes() {
        let input = &[0xF1, 0xF1, 0x05, 0x00, 0x05, 0x7E, 0xBA, 0xDE];
        assert_eq!(
            Packet::parse(input).unwrap(),
            (
                Packet {
                    target_address: 0xF1,
                    command: 0x05,
                    payload: vec![]
                },
                &[0xBAu8, 0xDEu8] as &[u8]
            )
        );
    }

    #[test]
    fn packet_parse_stream() {
        let input = &[
            0xBA, 0xDE, 0xF2, 0xF2, 0x01, 0x03, 0x01, 56, 0x07, 68, 0x7E, 0xF1, 0xF1, 0x05, 0x00,
            0x05, 0x7E, 0xBA, 0xDE,
        ];
        assert_eq!(
            Packet::parse_stream(input),
            vec![
                Packet {
                    target_address: 0xF2,
                    command: 0x01,
                    payload: vec![0x01, 56, 0x07]
                },
                Packet {
                    target_address: 0xF1,
                    command: 0x05,
                    payload: vec![]
                },
            ]
        );
    }

    #[test]
    fn packet_serialize() {
        let pkt = Packet {
            target_address: 0xF2,
            command: 0x01,
            payload: vec![0x01, 56, 0x07],
        };
        assert_eq!(
            pkt.serialize(),
            vec![0xF2, 0xF2, 0x01, 0x03, 0x01, 56, 0x07, 68, 0x7E]
        );
    }
}
