use common::errors::*;
use crypto::checksum::crc16::crc16_lut;

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
