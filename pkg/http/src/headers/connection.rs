use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::complete;
use parsing::opaque::OpaqueString;

use crate::header::CONNECTION;
use crate::header::{Header, Headers};
use crate::header_syntax::comma_delimited;
use crate::message::Version;
use crate::{message::HTTP_V1_1, message_syntax::parse_token};

const MAX_CONNECTION_OPTIONS: usize = 4;

// NOTE: Names are case insensitive.
const KEEP_ALIVE: &'static str = "Keep-Alive";
const CLOSE: &'static str = "Close";

pub enum ConnectionOption {
    // TODO: Also parse Keep-Alive related options.
    KeepAlive,

    Close,

    Unknown(AsciiString),
}

impl AsRef<[u8]> for ConnectionOption {
    fn as_ref(&self) -> &[u8] {
        match self {
            ConnectionOption::KeepAlive => KEEP_ALIVE.as_bytes(),
            ConnectionOption::Close => CLOSE.as_bytes(),
            ConnectionOption::Unknown(v) => v.as_ref().as_bytes(),
        }
    }
}

impl<T: AsRef<[u8]>> PartialEq<T> for ConnectionOption {
    fn eq(&self, other: &T) -> bool {
        self.as_ref().eq_ignore_ascii_case(other.as_ref())
    }
}

// `connection-option = token`
// parser!(parse_connection_option<AsciiString> => {
//     parse_token
// });

/// Syntax defined in RFC 7230 Section 6.1 as:
/// `Connection = 1#connection-option`
/// `connection-option = token`
///
/// TODO: Ideally we would return a set (and verify that a connection type isn't
/// specified twice.)
pub fn parse_connection(headers: &Headers) -> Result<Vec<ConnectionOption>> {
    let mut option_names = vec![];
    for header in headers.find(crate::header::CONNECTION) {
        let (names, _) = complete(comma_delimited(
            parse_token,
            1,
            MAX_CONNECTION_OPTIONS - option_names.len(),
        ))(header.value.to_bytes())?;
        option_names.extend(names.into_iter());
    }

    let mut options = vec![];
    for name in option_names {
        if name.eq_ignore_case(KEEP_ALIVE.as_bytes()) {
            options.push(ConnectionOption::KeepAlive)
        } else if name.eq_ignore_case(CLOSE.as_bytes()) {
            options.push(ConnectionOption::Close);
        } else {
            options.push(ConnectionOption::Unknown(name));
        }
    }

    // TODO: Implement support for the header options:
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Keep-Alive

    // NOTE: If a header corresponding to a connection option is received but the
    // option isn't mentioned in the 'Connection' header, then it must be
    // ignored.

    Ok(options)
}

/// Based on the HTTP version and Connection header, determines if the
/// connection can persistent. Algorithm from RFC 7230 Section 6.3
///
/// NOTE: If the body doesn't have a well defined length, then the connection
/// may have to close anyway.
///
/// Returns whether or not the connection can persist or an error if the request
/// is invalid.
pub fn can_connection_persist(received_version: &Version, headers: &Headers) -> Result<bool> {
    let options = parse_connection(headers)?;

    let mut has_close_option = false;
    let mut has_keep_alive_option = false;
    for option in &options {
        if option == &ConnectionOption::KeepAlive {
            has_keep_alive_option = true;
        } else if option == &ConnectionOption::Close {
            has_close_option = true;
        }
    }

    if has_close_option {
        return Ok(false);
    }

    // TODO: Technically this should be any version >= 1.1
    if received_version == &HTTP_V1_1 {
        return Ok(true);
    }

    // TODO: We must also not be a proxy.
    if has_keep_alive_option {
        return Ok(true);
    }

    Ok(false)
}

pub fn append_connection_header(persist_connection: bool, headers: &mut Headers) {
    headers.raw_headers.push(Header {
        name: AsciiString::from(Bytes::from(CONNECTION)).unwrap(),
        value: OpaqueString::from(if persist_connection {
            "keep-alive"
        } else {
            "close"
        }),
    });
}
