pub fn encode_html_text(s: &str) -> String {
    let mut out = String::new();
    out.reserve(s.len());

    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }

    out
}
