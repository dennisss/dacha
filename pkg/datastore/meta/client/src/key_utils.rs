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
