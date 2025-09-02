pub fn crc8(data: &[u8]) -> u8 {
    const INIT_REMAINDER: u8 = 0xff;
    const FINAL_XOR: u8 = 0x00;
    const POLYNOMIAL: u8 = 0x31; // x^8 + x^5 + x^4 + 1

    let mut state: u8 = INIT_REMAINDER;
    for byte in data {
        state ^= *byte;
        for _ in 0..8 {
            let overflow = state & (1 << 7) != 0;
            state <<= 1;
            if overflow {
                state ^= POLYNOMIAL;
            }
        }
    }

    state ^ FINAL_XOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_test() {
        assert_eq!(crc8(&[0xBE, 0xEF]), 0x92);
    }
}
