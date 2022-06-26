
#[derive(Debug, Fail)]
pub struct Errno(pub i64);

impl std::fmt::Display for Errno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}", self
        )
    }
}

/*

#[derive(Debug, Fail)]
pub(crate) struct ProtocolErrorV1 {
    pub code: StatusCode,

    /// NOTE: This isn't sent back to the HTTP client as there is no good method
    /// of doing that without sending a body (which may be misinterpreted by
    /// a client).
    pub message: &'static str,
}

impl std::fmt::Display for ProtocolErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}: {}] {}",
            self.code.as_u16(),
            self.code.default_reason().unwrap_or(""),
            self.message
        )
    }
}
*/