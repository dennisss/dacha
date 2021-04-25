use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::complete;

use crate::message_syntax::parse_token;
use crate::header::Headers;
use crate::header_syntax::comma_delimited;
use crate::message::Version;

const MAX_CONNECTION_OPTIONS: usize = 4;

// NOTE: Names are case insensitive.
const KEEP_ALIVE: &'static str = "Keep-Alive";
const CLOSE: &'static str = "Close";

pub enum ConnectionOption {
    // TODO: Also parse Keep-Alive related options. 
    KeepAlive,
    
    Close,
    
    Unknown(AsciiString)
}



// `connection-option = token`
// parser!(parse_connection_option<AsciiString> => {
//     parse_token
// });


/// Syntax defined in RFC 7230 Section 6.1 as:
/// `Connection = 1#connection-option`
/// `connection-option = token`
/// 
/// TODO: Ideally we would return a set (and verify that a connection type isn't specified twice.)
pub fn parse_connection(headers: &Headers) -> Result<Vec<ConnectionOption>> {
    let mut option_names = vec![];
    for header in headers.find(crate::header::CONNECTION) {
        let (names, _) = complete(comma_delimited(parse_token, 1, MAX_CONNECTION_OPTIONS - option_names.len()))
            (header.value.to_bytes())?;
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

    // NOTE: If a header corresponding to a connection option is received but the option isn't mentioned
    // in the 'Connection' header, then it must be ignored.

    Ok(options)
}

/// Based on the HTTP version and Connection header, determines if the connection can persistent.
/// Algorithm from RFC 7230 Section 6.3
/// 
/// NOTE: If the body doesn't have a well defined length, then the connection may have to close
/// anyway.
pub fn can_connection_persistent(version: &Version, headers: &Headers) -> Result<bool> {
    Ok(false)
}