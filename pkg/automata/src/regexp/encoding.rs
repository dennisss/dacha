/// Defines how regular expression executors will decode search streing bytes
/// into code points. The entire input string up to the end of the matched
/// portion must must conform to this encoding for a match to be successful.
///
/// NOTE: Regular expression patterns are always interprated as UTF-8 Rust
/// strings.
#[derive(Clone, Copy)]
pub enum CharacterEncoding {
    /// Treat every input byte as its own code point / symbol. All byte values
    /// from 0-255 are accepted by the '.' regular expression.
    Latin1,

    /// Treat every input byte as its own code point / symbol. All bytes in the
    /// input must be in the range 0-127 (otherwise matching will reject the
    /// entire string).
    ASCII,

    UTF8,
}
