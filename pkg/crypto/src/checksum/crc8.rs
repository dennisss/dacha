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

pub fn crc8_atm(data: &[u8]) -> u8 {
    const POLYNOMIAL: u8 =  0x07; // x^8 + x^2 + x^1 + x^0

    let mut state: u8 = 0;

    for mut byte in data.iter().cloned() {
        for _ in 0..8 {
            let overflow = ((state >> 7) ^ (byte & 0x01)) != 0;
            state <<= 1;
            if overflow {
                state ^= POLYNOMIAL;
            }

            byte = byte >> 1;
        }
    }

    state
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_test() {
        assert_eq!(crc8(&[0xBE, 0xEF]), 0x92);
    }
}
