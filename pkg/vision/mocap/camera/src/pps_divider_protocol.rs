use std::convert::TryInto;

use crypto::checksum::crc::CRC32Hasher;
use crypto::hasher::Hasher;

const SYNC_1: u8 = 0xAA;
const SYNC_2: u8 = 0x55;

const PKT_TYPE_CONFIG: u8 = 0x01;
const PKT_TYPE_ACK: u8 = 0x02;
const PKT_TYPE_TELEM: u8 = 0x03;
const PKT_TYPE_HEARTBEAT: u8 = 0x04;
const PKT_TYPE_GET_BUILD_ID: u8 = 0x05;
const PKT_TYPE_BUILD_ID: u8 = 0x06;
const PKT_TYPE_SETUP: u8 = 0x07;


#[derive(Debug, Clone, PartialEq)]
pub struct ConfigPayload {
    pub sequence: u8,
    pub unlock: u8,
    pub pulse_rate: u8,
    pub frame_width_ticks: u32,
    pub frame_offset_ticks: i32,
    pub strobe_offset_ticks: i32,
    pub strobe_width_ticks: u32,
    pub strobe_dimming: u16,
    pub rgb_color: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AckPayload {
    pub sequence: u8,
    pub status: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TelemetryPayload {
    pub width: u32,
    pub prediction_error: i32,
    pub output_errors: u32,
    pub config_sequence: u8,
    pub frequency_mhz: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeartbeatPayload {
    pub temp_c: f32,
    pub vcc_min_v: f32,
    pub vcc_max_v: f32,
    pub poe_min_v: f32,
    pub poe_max_v: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetBuildIdPayload {
    pub sequence: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildIdPayload {
    pub sequence: u8,
    pub build_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupPayload {
    pub sequence: u8,
    pub pcb_revision: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Config(ConfigPayload),
    Ack(AckPayload),
    Telemetry(TelemetryPayload),
    Heartbeat(HeartbeatPayload),
    GetBuildId(GetBuildIdPayload),
    BuildId(BuildIdPayload),
    Setup(SetupPayload),
}

impl Packet {
    /// Serializes the packet into the wire format:
    /// [SYNC1] [SYNC2] [LEN] [TYPE] [PAYLOAD...] [CRC]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::new();

        // 1. Serialize Body (TYPE + PAYLOAD)
        match self {
            Packet::Config(cfg) => {
                body.push(PKT_TYPE_CONFIG);
                body.push(cfg.sequence);
                body.push(cfg.unlock);
                body.push(cfg.pulse_rate);
                body.extend_from_slice(&cfg.frame_width_ticks.to_le_bytes());
                body.extend_from_slice(&cfg.frame_offset_ticks.to_le_bytes());
                body.extend_from_slice(&cfg.strobe_offset_ticks.to_le_bytes());
                body.extend_from_slice(&cfg.strobe_width_ticks.to_le_bytes());
                body.extend_from_slice(&cfg.strobe_dimming.to_le_bytes());
                body.extend_from_slice(&cfg.rgb_color.to_le_bytes());
            }
            Packet::Ack(ack) => {
                body.push(PKT_TYPE_ACK);
                body.push(ack.sequence);
                body.push(ack.status);
            }
            Packet::Telemetry(telem) => {
                body.push(PKT_TYPE_TELEM);
                body.extend_from_slice(&telem.width.to_le_bytes());
                body.extend_from_slice(&telem.prediction_error.to_le_bytes());
                body.extend_from_slice(&telem.output_errors.to_le_bytes());
                body.push(telem.config_sequence);
                body.push(telem.frequency_mhz);
            }
            Packet::Heartbeat(hb) => {
                body.push(PKT_TYPE_HEARTBEAT);
                // Dummy conversion back to ADC values (temp is now u8 half-degrees!)
                let temp_adc = (hb.temp_c * 2.0) as u8;
                let vcc_min_adc = hb.vcc_min_v / 3.3 * 4095.0;
                let vcc_max_adc = hb.vcc_max_v / 3.3 * 4095.0;
                let poe_min_adc = hb.poe_min_v / 3.3 * 4095.0;
                let poe_max_adc = hb.poe_max_v / 3.3 * 4095.0;
                
                body.push(temp_adc);
                body.extend_from_slice(&(vcc_min_adc as u16).to_le_bytes());
                body.extend_from_slice(&(vcc_max_adc as u16).to_le_bytes());
                body.extend_from_slice(&(poe_min_adc as u16).to_le_bytes());
                body.extend_from_slice(&(poe_max_adc as u16).to_le_bytes());
            }
            Packet::GetBuildId(req) => {
                body.push(PKT_TYPE_GET_BUILD_ID);
                body.push(req.sequence);
            }
            Packet::BuildId(resp) => {
                body.push(PKT_TYPE_BUILD_ID);
                body.push(resp.sequence);
                body.extend_from_slice(&resp.build_id.to_le_bytes());
            }
            Packet::Setup(setup) => {
                body.push(PKT_TYPE_SETUP);
                body.push(setup.sequence);
                body.extend_from_slice(&setup.pcb_revision.to_le_bytes());
            }
        }

        // 2. Construct Full Packet
        let mut packet = Vec::new();
        packet.push(SYNC_1);
        packet.push(SYNC_2);
        
        let len = body.len() as u8;
        packet.push(len);

        let mut crc_input = Vec::new();
        crc_input.push(len);
        crc_input.extend_from_slice(&body);
        
        let crc = stm32_crc32(&crc_input);

        packet.extend(body);
        packet.extend_from_slice(&crc.to_le_bytes());

        packet
    }
}

fn stm32_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;

    for byte in data.iter().cloned() {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            let overflow = (crc & (0b1 << 31)) != 0;
            crc <<= 1;
            if overflow {
                crc ^= 0x04C11DB7;
            }
        }
    }

    crc
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    WaitSync1,
    WaitSync2,
    ReadLen,
    ReadPayload,
    ReadCrc,
}

pub struct PacketParser {
    state: ParserState,
    buffer: Vec<u8>,       // Stores [LEN, TYPE, PAYLOAD...]
    expected_len: u8,      // Derived from the LEN byte
    payload_idx: usize,
}

impl PacketParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::WaitSync1,
            buffer: Vec::with_capacity(64),
            expected_len: 0,
            payload_idx: 0,
        }
    }

    /// Resets the parser state (e.g., on timeout).
    /// Discards any partially accumulated packet.
    pub fn reset(&mut self) {
        self.state = ParserState::WaitSync1;
        self.buffer.clear();
        self.expected_len = 0;
        self.payload_idx = 0;
    }

    /// Parses input data byte-by-byte.
    /// Returns: (Option<Packet>, bytes_consumed)
    /// 
    /// - `bytes_consumed`: How many bytes from `data` were processed.
    ///   The caller should advance their buffer by this amount.
    /// - `Option<Packet>`: A valid packet if one was completed.
    pub fn parse(&mut self, data: &[u8]) -> (Option<Packet>, usize) {
        let mut consumed = 0;

        for &b in data {
            consumed += 1;
            
            match self.state {
                ParserState::WaitSync1 => {
                    if b == SYNC_1 {
                        self.state = ParserState::WaitSync2;
                    }
                }
                ParserState::WaitSync2 => {
                    if b == SYNC_2 {
                        self.state = ParserState::ReadLen;
                    } else if b == SYNC_1 {
                        // Received AA AA, stay in WaitSync2
                        self.state = ParserState::WaitSync2;
                    } else {
                        self.state = ParserState::WaitSync1;
                    }
                }
                ParserState::ReadLen => {
                    // Maximum reasonable length check (e.g., 60 bytes)
                    if b > 60 {
                        // eprintln!("BAD LENGTH");

                        self.reset();
                    } else {
                        self.expected_len = b;
                        self.buffer.clear();
                        self.buffer.push(b); // Store LEN for CRC calc
                        self.state = ParserState::ReadPayload;
                        self.payload_idx = 0;
                    }
                }
                ParserState::ReadPayload => {
                    self.buffer.push(b);
                    self.payload_idx += 1;
                    
                    // We need 'expected_len' bytes of body (Type + Data)
                    if self.payload_idx >= self.expected_len as usize {
                        self.state = ParserState::ReadCrc;
                        self.payload_idx = 0; // Reset to count CRC bytes
                    }
                }
                ParserState::ReadCrc => {
                    self.buffer.push(b); // Temporarily store CRC
                    self.payload_idx += 1;

                    if self.payload_idx >= 4 {
                        // Packet Complete!
                        let result = self.finalize_packet();
                        self.reset(); // Reset for next packet
                        
                        if let Some(pkt) = result {
                            return (Some(pkt), consumed);
                        } else {
                            eprintln!("INVALID PACKET");
                        }
                        // If invalid CRC or malformed, we consumed bytes but return None.
                        // Loop continues if there are more bytes in 'data'.
                    }
                }
            }
        }

        (None, consumed)
    }

    fn finalize_packet(&self) -> Option<Packet> {
        // Buffer layout: [LEN, TYPE, ...PAYLOAD..., CRC(4)]
        let total_len = self.buffer.len();
        if total_len < 5 { return None; } // LEN + TYPE + CRC(4) is min 6? No, LEN(1)+TYPE(1)+CRC(4) = 6. 
                                          // Wait, Heartbeat is LEN=1 (Type), so buffer is LEN(1)+TYPE(1)+CRC(4) = 6.
        
        let data_len = total_len - 4; // Length excluding CRC bytes
        let crc_bytes = &self.buffer[data_len..];
        let rx_crc = u32::from_le_bytes(crc_bytes.try_into().unwrap());

        // Calculate CRC over [LEN, TYPE, PAYLOAD...]
        let calculated_crc = stm32_crc32(&self.buffer[0..data_len]);

        if calculated_crc != rx_crc {
            println!("BAD CRC");
            println!("{:?}", crc_bytes);
            println!("{:?}", calculated_crc.to_le_bytes());

            return None; // CRC Mismatch
        }

        // Parse Payload
        let pkt_type = self.buffer[1]; // Index 0 is LEN, Index 1 is TYPE
        let payload_bytes = &self.buffer[2..data_len];

        match pkt_type {
            PKT_TYPE_CONFIG => {
                if payload_bytes.len() != 25 { return None; }
                Some(Packet::Config(ConfigPayload {
                    sequence: payload_bytes[0],
                    unlock: payload_bytes[1],
                    pulse_rate: payload_bytes[2],
                    frame_width_ticks: u32::from_le_bytes(payload_bytes[3..7].try_into().ok()?),
                    frame_offset_ticks: i32::from_le_bytes(payload_bytes[7..11].try_into().ok()?),
                    strobe_offset_ticks: i32::from_le_bytes(payload_bytes[11..15].try_into().ok()?),
                    strobe_width_ticks: u32::from_le_bytes(payload_bytes[15..19].try_into().ok()?),
                    strobe_dimming: u16::from_le_bytes(payload_bytes[19..21].try_into().ok()?),
                    rgb_color: u32::from_le_bytes(payload_bytes[21..25].try_into().ok()?),
                }))
            }
            PKT_TYPE_ACK => {
                if payload_bytes.len() != 2 { return None; }
                Some(Packet::Ack(AckPayload {
                    sequence: payload_bytes[0],
                    status: payload_bytes[1],
                }))
            }
            PKT_TYPE_TELEM => {
                if payload_bytes.len() != 14 { return None; }
                Some(Packet::Telemetry(TelemetryPayload {
                    width: u32::from_le_bytes(payload_bytes[0..4].try_into().ok()?),
                    prediction_error: i32::from_le_bytes(payload_bytes[4..8].try_into().ok()?),
                    output_errors: u32::from_le_bytes(payload_bytes[8..12].try_into().ok()?),
                    config_sequence: payload_bytes[12],
                    frequency_mhz: payload_bytes[13],
                }))
            }
            PKT_TYPE_HEARTBEAT => {
                if payload_bytes.len() != 9 { return None; } // 1 + 2 + 2 + 2 + 2
                
                let temp_adc = payload_bytes[0];
                let vcc_min_adc = u16::from_le_bytes(payload_bytes[1..3].try_into().ok()?);
                let vcc_max_adc = u16::from_le_bytes(payload_bytes[3..5].try_into().ok()?);
                let poe_min_adc = u16::from_le_bytes(payload_bytes[5..7].try_into().ok()?);
                let poe_max_adc = u16::from_le_bytes(payload_bytes[7..9].try_into().ok()?);

                // Voltage (V) = (Value_Reported / 4095.0) * 3.3
                // Temperature is now raw half-degrees C in a single u8
                let temp_c = temp_adc as f32 / 2.0;

                Some(Packet::Heartbeat(HeartbeatPayload {
                    temp_c,
                    vcc_min_v: (vcc_min_adc as f32 / 4095.0) * 3.3,
                    vcc_max_v: (vcc_max_adc as f32 / 4095.0) * 3.3,
                    poe_min_v: (poe_min_adc as f32 / 4095.0) * 3.3,
                    poe_max_v: (poe_max_adc as f32 / 4095.0) * 3.3,
                }))
            }
            PKT_TYPE_GET_BUILD_ID => {
                if payload_bytes.len() != 1 { return None; }
                Some(Packet::GetBuildId(GetBuildIdPayload {
                    sequence: payload_bytes[0],
                }))
            }
            PKT_TYPE_BUILD_ID => {
                if payload_bytes.len() != 9 { return None; }
                Some(Packet::BuildId(BuildIdPayload {
                    sequence: payload_bytes[0],
                    build_id: u64::from_le_bytes(payload_bytes[1..9].try_into().ok()?),
                }))
            }
            PKT_TYPE_SETUP => {
                if payload_bytes.len() != 3 { return None; }
                Some(Packet::Setup(SetupPayload {
                    sequence: payload_bytes[0],
                    pcb_revision: u16::from_le_bytes(payload_bytes[1..3].try_into().ok()?),
                }))
            }
            _ => None,
        }
    }
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let pkt = Packet::Config(ConfigPayload {
            sequence: 42,
            unlock: 1,
            pulse_rate: 60,
            frame_width_ticks: 1000,
            frame_offset_ticks: 300,
            strobe_offset_ticks: -50,
            strobe_width_ticks: 200,
            strobe_dimming: 1000,
            rgb_color: 0x00FF00FF,
        });

        let bytes = pkt.to_bytes();
        
        // Header
        assert_eq!(bytes[0], SYNC_1);
        assert_eq!(bytes[1], SYNC_2);
        
        // LEN = 1 (Type) + 25 (Payload) = 26
        assert_eq!(bytes[2], 26); 
        assert_eq!(bytes[3], PKT_TYPE_CONFIG);
        assert_eq!(bytes[4], 42); // Seq

        // Parser Check
        let mut parser = PacketParser::new();
        let (parsed, consumed) = parser.parse(&bytes);
        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed, Some(pkt));
    }

    #[test]
    fn test_fragmented_packet() {
        let pkt = Packet::Telemetry(TelemetryPayload { width: 0, prediction_error: 123456, output_errors: 0, config_sequence: 1, frequency_mhz: 64 });
        let bytes = pkt.to_bytes();

        let mut parser = PacketParser::new();

        // Feed first half
        let (p1, c1) = parser.parse(&bytes[0..4]);
        assert_eq!(p1, None);
        assert_eq!(c1, 4);

        // Feed second half
        let (p2, c2) = parser.parse(&bytes[4..]);
        assert_eq!(p2, Some(pkt));
        assert_eq!(c2, bytes.len() - 4);
    }
}