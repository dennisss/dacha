use common::errors::*;

use crate::header::{Headers, RANGE};
use crate::uri::Authority;
use crate::uri_syntax::parse_authority;

pub fn parse_range_header(
    headers: &Headers,
    content_length: usize,
) -> Result<Option<(usize, usize)>> {
    let header = match headers.get_one(RANGE)? {
        Some(v) => v,
        None => return Ok(None),
    };

    let mut value = header.value.to_ascii_str()?;

    value = match value.strip_prefix("bytes=") {
        Some(v) => v,
        None => return Err(err_msg("Range header has unsupported units prefix")),
    };

    if value.contains(",") {
        return Err(err_msg("Multi-part ranges not supported"));
    }

    let (mut start, mut end) = value
        .split_once('-')
        .ok_or_else(|| err_msg("Missing - in range header"))?;

    start = start.trim();
    end = end.trim();

    let max_offset = if content_length > 0 {
        content_length - 1
    } else {
        0
    };

    let start_offset = start.parse::<usize>()?;

    let end_offset = {
        if end.is_empty() {
            max_offset
        } else {
            end.parse::<usize>()?
        }
    };

    if start_offset > max_offset || end_offset > max_offset {
        return Err(err_msg("Range out of bounds of content."));
    }

    if end_offset < start_offset {
        return Err(err_msg("End of range must be beyond the start"));
    }

    Ok(Some((start_offset, end_offset)))
}
