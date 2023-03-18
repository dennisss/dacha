use common::errors::*;

/// Finds the first occurence of the 'pattern' in 'data'.
///
/// TODO:
/// - For a 1 byte use a simple comparison.
/// - For a <=8 byte pattern, use simple u64 comparisons
/// - Otherwise, use Rabin-Karp
pub fn find_byte_pattern(data: &[u8], pattern: &[u8]) -> Option<usize> {
    assert!(!pattern.is_empty());

    if data.len() >= pattern.len() {
        for i in 0..(data.len() - pattern.len() + 1) {
            if &data[i..(i + pattern.len())] == pattern {
                return Some(i);
            }
        }
    }

    None
}

pub fn parse_pattern_terminated_bytes<'a>(
    data: &'a [u8],
    pattern: &[u8],
) -> Result<(&'a [u8], &'a [u8])> {
    let i = match find_byte_pattern(data, pattern) {
        Some(i) => i,
        None => {
            // TODO: Use an incomplete error?
            return Err(err_msg("Couldn't find pattern in data."));
        }
    };

    let v = &data[0..i];
    let rest = &data[(i + pattern.len())..];
    Ok((v, rest))
}
