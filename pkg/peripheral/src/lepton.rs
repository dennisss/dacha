use common::errors::*;

use crate::spi::{SPIDevice, SPI};

/*

I2C
- Only 16-bit transfers allows
- Address: 0x2A
- See 'Table 6' in the data sheet


VoSPI
- Lepton transmits Discard packets if no frame is available.
- Big-endian / MSB first
- SPI Mode 3 (CPOL=1, CPHA=1)
- Chip Select Active Low
- Max Clock: 20MHz


Video Output:
- Raw14 (or RGB)

- Lepton 3.5: 160h x 120v pixels
    - Horizontal FOV is 57 degrees

- Packet: 164 bytes (63 per frame with 3 of these being telemetry?)
    - Header:
        - 2 byte ID
            - xNNN hex for video frames
            - xFxx hex for discard frames
        - 2 byte CRC
            Poly: x^16 + x^12 + x^5 + x^0
            "The CRC is calculated over the entire packet, including the ID and CRC fields. However, the four most-significant
bits of the ID and all sixteen bits of the CRC are set to zero for calculation of the CRC"

- For Lepton 3.5, top-4 bits of the id are the segment number from 1-4
    - One frame is 4 segments (each has its own telemetry packet)
    - 240 packets for an entire image

Segment is only valid on packet #20


- Enable Pin Aative High

Seeing 60 frames (final number: 0x3B)

*/

pub struct Lepton {
    spi: SPIDevice,
}

impl Lepton {
    pub fn open(spi_path: &str) -> Result<Self> {
        let mut spi = SPIDevice::open(spi_path)?;
        // TODO: Should this be 3 or 1
        // Lepton uses 1? https://github.com/danjulio/lepton/blob/master/teensy3/lep_test9/lep_test9.ino#L354
        spi.set_mode(1)?;
        spi.set_speed_hz(20_000_000)?;

        Ok(Self { spi })
    }

    pub fn read_frame(&mut self) -> Result<Vec<u8>> {
        const PACKETS_PER_SEGMENT: u16 = 60;

        // In Raw14, it's 2 bytes per pixel
        // The length of this is also equal to 4 segments each with 60 packets each with
        // 160 data bytes.
        let mut frame_buffer = vec![0u8; 2 * 160 * 120];

        // Current index into the frame_buffer.
        let mut frame_i = 0;

        // NOTE: This will only range from 1-4 for valid packets.
        let mut last_segment_num = 0;

        let mut last_packet_num = PACKETS_PER_SEGMENT - 1;

        let mut packet_buffer = vec![0u8; 164];

        // TODO: Also enforce a timeout for completing a started frame and reading a
        // single frame in general.
        loop {
            // TODO: Start transfering the next line while the current one is being
            // processed? (use dual buffers).
            self.spi.transfer(&[], &mut &mut packet_buffer)?;

            let is_discard_pkt = (packet_buffer[0] & 0x0F) == 0x0F;
            if is_discard_pkt {
                // TODO: If we see this in the middle of a segment, reset everything.
                // We should only ever get this stuff
                continue;
            }

            // Upper 4 bits.
            let segment_num = packet_buffer[0] >> 4;
            // Next 12 bits.
            let packet_num = u16::from_be_bytes(*array_ref![packet_buffer, 0, 2]) & ((1 << 12) - 1);

            let crc = u16::from_be_bytes(*array_ref![packet_buffer, 2, 2]);

            let expected_packet_num = (last_packet_num + 1) % PACKETS_PER_SEGMENT;
            let expected_segment = last_segment_num + if expected_packet_num == 0 { 1 } else { 0 };

            // Prepare for checksuming the packet.
            // NOTE: We will only checksum the packet if all other values check out.
            packet_buffer[0] &= 0x0F;
            packet_buffer[2] = 0;
            packet_buffer[3] = 0;

            // NOTE: the segment number is only expected to be valid on packet 20.
            let valid = (expected_packet_num == packet_num)
                // Top 1 bit must always be a zero.
                && (segment_num & 0b1000 == 0)
                && (packet_num != 20 || expected_segment == segment_num)
                && (crc16_lut(&packet_buffer) == crc);
            if !valid {
                // Reset everything
                frame_i = 0;
                last_segment_num = 0;
                last_packet_num = PACKETS_PER_SEGMENT - 1;
                continue;
            }

            last_segment_num = expected_segment;
            last_packet_num = expected_packet_num;

            frame_buffer[frame_i..(frame_i + 160)].copy_from_slice(&packet_buffer[4..]);
            frame_i += 160;

            if frame_i == frame_buffer.len() {
                break;
            }
        }

        Ok(frame_buffer)
    }
}

// CRC-CCITT-16
fn crc16(data: &[u8]) -> u16 {
    const INIT_REMAINDER: u16 = 0;
    const FINAL_XOR: u16 = 0x00;
    const POLYNOMIAL: u16 = (1 << 12) | (1 << 5) | (1 << 0); // x^16 + x^12 + x^5 + x^0

    let mut state: u16 = INIT_REMAINDER;
    for byte in data {
        state ^= (*byte as u16) << 8;
        for _ in 0..8 {
            let overflow = state & (1 << 15) != 0;
            state <<= 1;
            if overflow {
                state ^= POLYNOMIAL;
            }
        }
    }

    state ^ FINAL_XOR
}

// Derived with the crc16_derive_lut_test test.
const CRC16_LUT: [u16; 256] = [
    0x00, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7, 0x8108, 0x9129, 0xa14a, 0xb16b,
    0xc18c, 0xd1ad, 0xe1ce, 0xf1ef, 0x1231, 0x210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6,
    0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de, 0x2462, 0x3443, 0x420, 0x1401,
    0x64e6, 0x74c7, 0x44a4, 0x5485, 0xa56a, 0xb54b, 0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d,
    0x3653, 0x2672, 0x1611, 0x630, 0x76d7, 0x66f6, 0x5695, 0x46b4, 0xb75b, 0xa77a, 0x9719, 0x8738,
    0xf7df, 0xe7fe, 0xd79d, 0xc7bc, 0x48c4, 0x58e5, 0x6886, 0x78a7, 0x840, 0x1861, 0x2802, 0x3823,
    0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b, 0x5af5, 0x4ad4, 0x7ab7, 0x6a96,
    0x1a71, 0xa50, 0x3a33, 0x2a12, 0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a,
    0x6ca6, 0x7c87, 0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0xc60, 0x1c41, 0xedae, 0xfd8f, 0xcdec, 0xddcd,
    0xad2a, 0xbd0b, 0x8d68, 0x9d49, 0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0xe70,
    0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78, 0x9188, 0x81a9, 0xb1ca, 0xa1eb,
    0xd10c, 0xc12d, 0xf14e, 0xe16f, 0x1080, 0xa1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e, 0x2b1, 0x1290, 0x22f3, 0x32d2,
    0x4235, 0x5214, 0x6277, 0x7256, 0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
    0x34e2, 0x24c3, 0x14a0, 0x481, 0x7466, 0x6447, 0x5424, 0x4405, 0xa7db, 0xb7fa, 0x8799, 0x97b8,
    0xe75f, 0xf77e, 0xc71d, 0xd73c, 0x26d3, 0x36f2, 0x691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9, 0xb98a, 0xa9ab, 0x5844, 0x4865, 0x7806, 0x6827,
    0x18c0, 0x8e1, 0x3882, 0x28a3, 0xcb7d, 0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a,
    0x4a75, 0x5a54, 0x6a37, 0x7a16, 0xaf1, 0x1ad0, 0x2ab3, 0x3a92, 0xfd2e, 0xed0f, 0xdd6c, 0xcd4d,
    0xbdaa, 0xad8b, 0x9de8, 0x8dc9, 0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0xcc1,
    0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8, 0x6e17, 0x7e36, 0x4e55, 0x5e74,
    0x2e93, 0x3eb2, 0xed1, 0x1ef0,
];

fn crc16_lut(data: &[u8]) -> u16 {
    let mut state: u16 = 0;
    for byte in data {
        state ^= (*byte as u16) << 8;
        let upper = state >> 8;
        state = (state << 8) ^ CRC16_LUT[upper as usize];
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_test() {
        assert_eq!(crc16(&[0, 0, 0]), 0);
        assert_eq!(crc16(&[1]), 0x1021);
        assert_eq!(crc16(&[0x6e]), 0x8D68);
        assert_eq!(crc16(&[0xAA, 0xBB, 0xCC, 0xDD]), 0xC53A);
        assert_eq!(crc16(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]), 0xC360);
    }

    #[test]
    fn crc16_lut_test() {
        assert_eq!(crc16_lut(&[0, 0, 0]), 0);
        assert_eq!(crc16_lut(&[1]), 0x1021);
        assert_eq!(crc16_lut(&[0x6e]), 0x8D68);
        assert_eq!(crc16_lut(&[0xAA, 0xBB, 0xCC, 0xDD]), 0xC53A);
        assert_eq!(crc16_lut(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]), 0xC360);
    }

    #[test]
    fn crc16_derive_lut_test() {
        let mut lut = vec![];
        for i in 0..=255 {
            // This mainly works as these is no init remainder or final xor.
            lut.push(crc16(&[i]));
        }

        println!("{:#04x?}", &lut);
    }

    #[test]
    fn crc16_benchmark_test() {
        let input_data = std::fs::read(project_path!("testdata/random/random_463")).unwrap();
        let iters = 1000000;

        {
            let start_time = std::time::Instant::now();
            let mut i = 0;
            for _ in 0..iters {
                i += crc16(&input_data);
            }
            let end_time = std::time::Instant::now();
            println!(
                "CRC16 Took: {}ms {}",
                end_time.duration_since(start_time).as_millis(),
                i
            );
        }

        {
            let start_time = std::time::Instant::now();

            let mut i = 0;
            for _ in 0..iters {
                i += crc16_lut(&input_data);
            }
            let end_time = std::time::Instant::now();
            println!(
                "CRC16 LUT Took: {}ms {}",
                end_time.duration_since(start_time).as_millis(),
                i
            );
        }
    }
}
