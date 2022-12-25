use alloc::string::String;

const U64_NUM_BYTES: usize = 13; // crate::ceil_div(64, 5);

/*
Encodings Supported:
- CL64
    - Encodes only 64-bit integers
    - Uses the Douglas Crockford dictionary (https://www.crockford.com/base32.html).
    - Always outputs 13 bytes.
    - The first encoded character is always alphabetic.
    - Case insensitive encoding.
*/

pub fn base32_encode_cl64(mut num: u64) -> String {
    const ENCODE_MAP: &'static [u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

    let mut out = String::new();

    let mut first_code = (num & 0b1111) as u8;
    num >>= 4;
    if first_code < 10 {
        first_code |= 1 << 4;
    }
    out.push(ENCODE_MAP[first_code as usize] as char);

    for _ in 0..(U64_NUM_BYTES - 1) {
        let code = (num & 0b11111) as u8;
        num >>= 5;

        out.push(ENCODE_MAP[code as usize] as char);
    }

    out
}

pub fn base32_decode_cl64<T: AsRef<[u8]>>(data: T) -> Option<u64> {
    let data = data.as_ref();

    fn decode_char(v: u8) -> Option<u64> {
        Some(match v {
            b'0' | b'o' | b'O' => 0,
            b'1' | b'I' | b'i' | b'L' | b'l' => 1,
            b'2' => 2,
            b'3' => 3,
            b'4' => 4,
            b'5' => 5,
            b'6' => 6,
            b'7' => 7,
            b'8' => 8,
            b'9' => 9,
            b'a' | b'A' => 10,
            b'b' | b'B' => 11,
            b'c' | b'C' => 12,
            b'd' | b'D' => 13,
            b'e' | b'E' => 14,
            b'f' | b'F' => 15,
            b'g' | b'G' => 16,
            b'h' | b'H' => 17,
            b'j' | b'J' => 18,
            b'k' | b'K' => 19,
            b'm' | b'M' => 20,
            b'n' | b'N' => 21,
            b'p' | b'P' => 22,
            b'q' | b'Q' => 23,
            b'r' | b'R' => 24,
            b's' | b'S' => 25,
            b't' | b'T' => 26,
            b'v' | b'V' => 27,
            b'w' | b'W' => 28,
            b'x' | b'X' => 29,
            b'y' | b'Y' => 30,
            b'z' | b'Z' => 31,
            _ => {
                return None;
            }
        })
    }

    if data.len() != U64_NUM_BYTES {
        return None;
    }

    let mut out = 0;
    for i in (1..U64_NUM_BYTES).rev() {
        out <<= 5;

        let code = match decode_char(data[i]) {
            Some(v) => v,
            None => {
                return None;
            }
        };

        out |= code;
    }

    out <<= 4;
    let first_code = match decode_char(data[0]) {
        Some(v) => v,
        None => {
            return None;
        }
    };
    out |= first_code & 0b1111;

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cl64_encoding() {
        for i in 0..5000 {
            let v = base32_encode_cl64(i);
            let v2 = base32_decode_cl64(&v).unwrap();
            let v3 = base32_decode_cl64(v.to_uppercase()).unwrap();
            assert_eq!(v2, i);
            assert_eq!(v3, i);
        }

        let nums: &[u64] = &[
            0x650852fa1b058b5f,
            0xf2a87b67fed0338e,
            0x0bac916ae46ed0ee,
            0xffffffffffffffff,
        ];

        for i in nums.iter().cloned() {
            let v = base32_encode_cl64(i);
            let v2 = base32_decode_cl64(&v).unwrap();
            let v3 = base32_decode_cl64(v.to_uppercase()).unwrap();
            assert_eq!(v2, i);
            assert_eq!(v3, i);

            println!("{}", v);
        }
    }
}
