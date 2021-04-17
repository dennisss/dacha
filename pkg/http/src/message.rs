use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::reader::*;
use crate::uri::*;
use crate::header::*;

/// Buffering options to use when reading the head of an HTTP request/response.
pub const MESSAGE_HEAD_BUFFER_OPTIONS: StreamBufferOptions = StreamBufferOptions {
    max_buffer_size: 256*1024,  // 256KB
    buffer_size: 1024,
};

// TODO: Pass these into the read_matching()
// const buffer_size: usize = 1024;

// If we average an http preamable (request line + headers) larger than this
// size, then we will fail the request. const max_buffer_size: usize = 16*1024;
// // 16KB

// TODO: Read https://www.ietf.org/rfc/rfc1945.txt, we should never actually see 0.9 in a one-liner

pub const HTTP_V0_9: Version = Version { major: 0, minor: 9 };
pub const HTTP_V1_0: Version = Version { major: 1, minor: 0 };
pub const HTTP_V1_1: Version = Version { major: 1, minor: 1 };
pub const HTTP_V2_0: Version = Version { major: 2, minor: 0 };

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl Version {
    pub fn to_string(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

#[derive(Debug)]
pub(crate) struct HttpMessageHead {
    pub start_line: StartLine,
    pub headers: Headers,
}

#[derive(Debug)]
pub(crate) enum StartLine {
    Request(RequestLine),
    Response(StatusLine),
}

#[derive(Debug)]
pub(crate) struct RequestLine {
    pub method: AsciiString,
    pub target: RequestTarget,
    pub version: Version,
}

#[derive(Debug)]
pub(crate) struct StatusLine {
    pub version: Version,
    pub status_code: u16,
    pub reason: OpaqueString,
}

// https://tools.ietf.org/html/rfc7230#section-5.3
#[derive(Debug)]
pub enum RequestTarget {
    /// Standard relative path. This is the typical request
    OriginForm(Vec<OpaqueString>, Option<OpaqueString>),

    /// Typically a proxy request
    /// NOTE: Must be accepted ALWAYS be servers.
    AbsoluteForm(Uri),

    /// Only used for CONNECT.
    AuthorityForm(Authority),

    /// Used for OPTIONS.
    AsteriskForm,
}

impl RequestTarget {
    pub fn into_uri(self) -> Uri {
        match self {
            RequestTarget::OriginForm(path_abs, query) => Uri {
                scheme: None,
                authority: None,
                path: UriPath::Absolute(path_abs).to_opaque_string(),
                query,
                fragment: None,
            },
            RequestTarget::AbsoluteForm(u) => u,
            RequestTarget::AuthorityForm(a) => Uri {
                scheme: None,
                authority: Some(a),
                // TODO: Wrong?
                path: OpaqueString::new(),
                query: None,
                fragment: None,
            },
            RequestTarget::AsteriskForm => Uri {
                scheme: None,
                authority: None,
                path: OpaqueString::from("*"),
                query: None,
                fragment: None,
            },
        }
    }
}



pub enum HttpStreamEvent {
    /// Read the entire head of a http request/response message.
    /// The raw bytes of the head will be the first item of this tuple.
    /// All remaining bytes after the head are stored in the second item.
    MessageHead(Bytes),

    /// The size of the message status line + headers is too large to fit into
    /// the internal buffers.
    HeadersTooLarge,

    /// The end of the TCP stream was hit, but there are left over bytes that
    /// don't represent a complete message.
    Incomplete(Bytes),

    /// The end of the Tcp stream was hit without any other messages being read.
    EndOfStream,
}

pub async fn read_http_message<'a>(stream: &mut StreamReader) -> Result<HttpStreamEvent> {
    // TODO: When we get the *first* CRLF, check if we got a 0.9 request
    // ^ TODO: Should only do the above when processing a request (it would be
    // invalid during a response).

    let val = stream.read_matching(LineMatcher::empty()).await?;
    Ok(match val {
        StreamReadUntil::Value(buf) => HttpStreamEvent::MessageHead(buf),
        StreamReadUntil::TooLarge => HttpStreamEvent::HeadersTooLarge,
        StreamReadUntil::Incomplete(b) => HttpStreamEvent::Incomplete(b),
        StreamReadUntil::EndOfStream => HttpStreamEvent::EndOfStream,
    })
}
