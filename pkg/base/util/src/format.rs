use alloc::string::String;

pub fn format_bytes(data: &[u8]) -> String {
    let mut out = String::new();
    for b in data {
        if b.is_ascii_alphanumeric() || b.is_ascii_punctuation() || *b == b' ' {
            out.push(*b as char);
        } else {
            out.push_str(&format!("\\x{:X}", *b))
        }
    }

    out
}
