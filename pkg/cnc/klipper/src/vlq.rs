// Written by Google Gemini
// https://gemini.google.com/app/1819ea07a42f1c48

use common::errors::*;

/// Encodes an i32 into a Klipper-compatible Big-Endian VLQ.
/// Matches Klipper's `PT_uint32.encode` behavior.
pub fn klipper_encode_vlq(v: i32, buffer: &mut Vec<u8>) {
    // Klipper writes in Big-Endian order (MSB first).
    // It checks thresholds to determine if higher-order bytes are needed.
    // The 0x80 bit is the continuation flag.

    if v >= 0xc000000 || v < -0x4000000 {
        buffer.push(((v >> 28) & 0x7f | 0x80) as u8);
    }
    if v >= 0x180000 || v < -0x80000 {
        buffer.push(((v >> 21) & 0x7f | 0x80) as u8);
    }
    if v >= 0x3000 || v < -0x1000 {
        buffer.push(((v >> 14) & 0x7f | 0x80) as u8);
    }
    if v >= 0x60 || v < -0x20 {
        buffer.push(((v >> 7) & 0x7f | 0x80) as u8);
    }

    // Write the least significant 7 bits (final byte, no 0x80)
    buffer.push((v & 0x7f) as u8);
}

/// Decodes a Klipper-compatible VLQ from a byte slice.
/// Matches Klipper's `PT_uint32.parse` behavior.
pub fn klipper_decode_vlq(data: &[u8]) -> Result<(i32, usize)> {
    if data.is_empty() {
        return Err(err_msg("Empty buffer"));
    }

    let mut pos = 0;
    
    // 1. Read the first byte (Most Significant Byte in this stream)
    let mut c = data[pos];
    pos += 1;

    // 2. Extract data bits
    let mut v = (c & 0x7f) as i32;

    // 3. Handle Sign Extension (The "Secret Sauce")
    // If bits 5 and 6 are set (0x60), Klipper treats this as a negative number start.
    // We must sign-extend `v` manually because we are building it from u8 parts.
    if (c & 0x60) == 0x60 {
        // In Python: v |= -0x20
        // In Rust i32: Mask upper bits to 1. (-32 is ...111111100000)
        v |= !0x1f; 
    }

    // 4. Loop while the *previous* byte had the continuation bit (0x80)
    while (c & 0x80) != 0 {
        if pos >= data.len() {
            return Err(err_msg("Buffer underflow: incomplete VLQ"));
        }
        
        c = data[pos];
        pos += 1;

        // Shift existing value left by 7 and add new bits
        v = (v << 7) | ((c & 0x7f) as i32);
    }

    Ok((v, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_klipper_positive() {
        let mut buf = Vec::new();
        // 300 -> 0x12C
        // >= 0x60, so 2 bytes.
        // Byte 1: (300 >> 7) | 0x80 = 2 | 0x80 = 0x82
        // Byte 2: 300 & 0x7f = 0x2C
        klipper_encode_vlq(300, &mut buf);
        assert_eq!(buf, vec![0x82, 0x2C]);
        
        let (val, len) = klipper_decode_vlq(&buf).unwrap();
        assert_eq!(val, 300);
        assert_eq!(len, 2);
    }

    #[test]
    fn test_klipper_negative() {
        let mut buf = Vec::new();
        // -5 -> ...11111011
        // -5 is NOT < -0x20 (-32). So it fits in 1 byte.
        // Byte 1: -5 & 0x7f = 0x7B (123)
        klipper_encode_vlq(-5, &mut buf);
        assert_eq!(buf, vec![0x7B]);

        // Decode check:
        // 0x7B is 1111 0111.
        // (0x7B & 0x60) is 0x60? Yes. (Bits 5,6 are 1)
        // Sign extend -> Result -5.
        let (val, len) = klipper_decode_vlq(&buf).unwrap();
        assert_eq!(val, -5);
        assert_eq!(len, 1);
    }

    #[test]
    fn test_klipper_large_negative() {
        let mut buf = Vec::new();
        // -100
        // < -32, so 2 bytes.
        // Byte 1: (-100 >> 7) & 0x7f | 0x80 
        //         -1 = ...111111
        //         -1 | 0x80 = 0xFF
        // Byte 2: -100 & 0x7f = 0x1C
        klipper_encode_vlq(-100, &mut buf);
        assert_eq!(buf, vec![0xFF, 0x1C]);

        let (val, len) = klipper_decode_vlq(&buf).unwrap();
        assert_eq!(val, -100);
    }
}