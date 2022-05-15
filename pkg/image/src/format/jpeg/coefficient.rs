/// Decodes a sign-amplitude stored value associated with an AC/DC coefficient.
///
/// The format is:
/// - Overall N bits (size argument)
/// - First bit is the sign
/// - If sign is 1, then the number is positive and the remaining bits store the
///   value.
/// - Else the value is negative.
///
/// TODO: These can be very large. Check that they don't cause out of range
/// multiplications. NOTE: Only works if size < 16.
/// TODO: Rename decode_amplitude?
pub fn decode_zz(size: usize, amplitude: u16) -> i16 {
    let sign = (amplitude >> ((size as u16) - 1)) & 0b1;
    if sign == 1 {
        // It is positive
        return amplitude as i16;
    }

    let extended = (0xffff_u16).overflowing_shl(size as u32).0 | amplitude;

    (extended as i16) + 1
}

pub fn encode_zz(value: i16) -> (usize, u16) {
    if value >= 0 {
        let size = 16 - value.leading_zeros();
        return (size as usize, value as u16);
    }

    let v = (value - 1) as u16;
    let size = 16 - v.leading_ones();

    (size as usize, v & ((1 << size) - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_zz_works() {
        assert_eq!(decode_zz(2, 0b00), -3);
        assert_eq!(decode_zz(2, 0b01), -2);
        assert_eq!(decode_zz(2, 0b10), 2);
        assert_eq!(decode_zz(2, 0b11), 3);

        assert_eq!(encode_zz(-3), (2, 0b00));
        assert_eq!(encode_zz(-2), (2, 0b01));
        assert_eq!(encode_zz(2), (2, 0b10));
        assert_eq!(encode_zz(3), (2, 0b11));
    }
}
