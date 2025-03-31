use common::bytes::Bytes;

pub fn single_key_range(key: &[u8]) -> (Bytes, Bytes) {
    let start_key = Bytes::from(key);
    let end_key = Bytes::from({
        let mut data = key.to_vec();
        data.push(0);
        data
    });

    (start_key, end_key)
}

pub fn prefix_key_range(prefix: &[u8]) -> (Bytes, Bytes) {
    let start_key = prefix.to_vec();
    let end_key = find_short_successor(start_key.clone());
    (start_key.into(), end_key.into())
}

// TODO: Dedup with sstable::table::BytewiseComparator
pub fn find_short_successor(mut key: Vec<u8>) -> Vec<u8> {
    for i in (0..key.len()).rev() {
        if key[i] != 0xff {
            key[i] += 1;
            key.truncate(i + 1);
            break;
        }
    }

    key
}

/// Given a range of keys, gets the longest key prefix that contains all of the
/// keys in the range.
///
/// (this is basically the opposite of prefix_key_range).
pub fn key_range_prefix<'a>(start: &'a [u8], end: &[u8]) -> &'a [u8] {
    let mut i = 0;

    if end <= start {
        return &[];
    }

    while i < start.len() {
        let start_i = start[i];

        let end_i = {
            if i == end.len() - 1 {
                // NOTE: This shouldn't overflow since 'end > start'
                end[i] - 1
            } else if i < end.len() {
                end[i]
            } else {
                0xff
            }
        };

        if start_i != end_i {
            break;
        }

        i += 1;
    }

    &start[..i]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_range_prefix_test() {
        let test_cases: &'static [(&'static [u8], &'static [u8], &'static [u8])] = &[
            //
            (&[0, 0], &[0, 1], &[0, 0]),
            (&[10, 0], &[10, 1], &[10, 0]),
            (&[1, 2, 3, 0], &[1, 2, 3, 2], &[1, 2, 3]),
            (&[1, 2, 3, 0xff], &[1, 2, 4], &[1, 2, 3, 0xff]),
            (&[1, 2, 0xff, 0xff], &[1, 3], &[1, 2, 0xff, 0xff]),
        ];

        for (ref a, ref b, ref c) in test_cases {
            assert_eq!(key_range_prefix(a, b), *c);
        }
    }
}
