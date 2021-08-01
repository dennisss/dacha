use common::chrono::prelude::*;
use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::header::{Header, Headers, DATE};

// Format defined in RFC 7231 Section 7.1.1.1

// Sun, 06 Nov 1994 08:49:37 GMT    ; IMF-fixdate
const TIME_FORMAT: &'static str = "%a, %d %b %Y %H:%M:%S GMT";

// Sunday, 06-Nov-94 08:49:37 GMT   ; obsolete RFC 850 format
const LEGACY_RFC_850_TIME_FORMAT: &'static str = "%A, %d-%b-%y %H:%M:%S %Z";

// Sun Nov  6 08:49:37 1994         ; ANSI C's asctime() format
const LEGACY_ASCTIME_FORMAT: &'static str = "%a %b %e %H:%M:%S %Y";

const TIME_FORMATS: &'static [&'static str] = &[
    TIME_FORMAT,
    LEGACY_RFC_850_TIME_FORMAT,
    LEGACY_ASCTIME_FORMAT,
];

pub fn append_current_date(headers: &mut Headers) {
    if headers.has(DATE) {
        return;
    }

    let now = Utc::now();
    let timestr = now.format(TIME_FORMAT).to_string();

    headers.raw_headers.push(Header {
        name: AsciiString::from(DATE).unwrap(),
        value: OpaqueString::from(timestr),
    });
}

pub fn parse_date(headers: &mut Headers) -> Result<Option<DateTime<Utc>>> {
    let mut iter = headers.find(DATE);

    let header = match iter.next() {
        Some(header) => header,
        None => {
            return Ok(None);
        }
    };

    if iter.next().is_some() {
        return Err(err_msg("More than one Date header"));
    }

    let value = header.value.to_ascii_str()?;

    for format in TIME_FORMATS {
        if let Ok(date) = DateTime::parse_from_str(value, TIME_FORMAT) {
            return Ok(Some(date.into()));
        }
    }

    Err(err_msg("Unknown Date header time format"))
}
