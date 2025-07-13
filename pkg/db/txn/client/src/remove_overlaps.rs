
use db_txn_proto::db::txn::*;


// TODO: Dedup with the regex symbol set code.
pub fn remove_overlaps(key_ranges: &mut [KeyRange]) -> Vec<KeyRange> {
    key_ranges.sort_by(|a, b| {
        // NOTE: The end_key ordering is intentionally reversed to reduce the number of times we expect to change the end_key.
        (a.start_key(), b.end_key()).cmp(&(b.start_key(), a.end_key()))
    });

    let mut out = vec![];
    out.reserve(key_ranges.len());

    for range in key_ranges {
        if range.start_key() == range.end_key() {
            continue;
        }

        let last_range = match out.last_mut() {
            Some(v) => v,
            None => {
                // TODO: Avoid copies and just re-use the input buffers.
                out.push(range.clone());
                continue;
            }
        };

        if range.start_key() <= last_range.end_key() {
            // last_range.end_key = max(last_range.end_key, range.end_key)
            if range.end_key() > last_range.end_key() {
                last_range.set_end_key(range.end_key());
            }

            continue;
        }

        out.push(range.clone());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {

        fn check(inputs: &[(&[u8], &[u8])], expected: &[(&[u8], &[u8])]) {
            let mut input_vec = vec![];
            for (a, b) in inputs {
                let mut v = KeyRange::default();
                v.set_start_key(*a);
                v.set_end_key(*b);
                input_vec.push(v)
            }

            let mut expected_vec = vec![];
            for (a, b) in expected {
                let mut v = KeyRange::default();
                v.set_start_key(*a);
                v.set_end_key(*b);
                expected_vec.push(v)
            }

            assert_eq!(remove_overlaps(&mut input_vec), expected_vec);
        }

        check(
            &[
                (b"a", b"b")
            ],
            &[
                (b"a", b"b")
            ],
        );
        check(
            &[
                (b"a", b"b"),
                (b"a", b"c"),
            ],
            &[
                (b"a", b"c")
            ],
        );
        check(
            &[
                (b"a", b"b"),
                (b"b", b"c"),
            ],
            &[
                (b"a", b"c")
            ],
        );
        check(
            &[
                (b"a", b"b"),
                (b"b", b"c"),
                (b"g", b"h"),
            ],
            &[
                (b"a", b"c"),
                (b"g", b"h"),
            ],
        );
        check(
            &[
                (b"a", b"i"),
                (b"b", b"w"),
                (b"x", b"y"),
            ],
            &[
                (b"a", b"w"),
                (b"x", b"y"),
            ],
        );
        check(
            &[
                (b"a", b"z"),
                (b"b", b"c"),
                (b"c", b"d"),
            ],
            &[
                (b"a", b"z"),
            ],
        );
    }

}