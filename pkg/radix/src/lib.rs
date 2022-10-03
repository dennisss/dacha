// TODO: Return Result<..>
pub fn hex_decode(text: &str) -> Vec<u8> {
    let mut out = vec![];

    let mut digit = String::new();
    let mut num_chars = 0;

    for c in text.chars() {
        digit.push(c);
        num_chars += 1;

        if num_chars == 2 {
            out.push(u8::from_str_radix(&digit, 16).unwrap());
            digit.clear();
            num_chars = 0;
        }
    }

    assert_eq!(num_chars, 0);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decode_test() {
        assert_eq!(&hex_decode("AB"), &[0xAB]);
        assert_eq!(&hex_decode("12"), &[0x12]);
        assert_eq!(&hex_decode("aabb"), &[0xAA, 0xBB]);
        assert_eq!(&hex_decode("123456789A"), &[0x12, 0x34, 0x56, 0x78, 0x9A]);
    }
}
