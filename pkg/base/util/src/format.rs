use alloc::string::String;

pub fn format_bytes(data: &[u8]) -> String {
    let mut out = String::new();
    for b in data.iter().cloned() {
        if b.is_ascii_alphanumeric() || b.is_ascii_punctuation() || b == b' ' {
            out.push(b as char);
        } else {
            if b == b'\\' {
                out.push_str("\\\\");
            } else if b == b'\r' {
                out.push_str("\\r");
            } else if b == b'\n' {
                out.push_str("\\n");
            } else if b == b'\t' {
                out.push_str("\\t");
            } else {
                out.push_str(&format!("\\x{:02X}", b))
            }
        }
    }

    out
}
