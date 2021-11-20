use common::bytes::Bytes;
use sstable::table::KeyComparator;

pub fn single_key_range(key: &[u8]) -> (Bytes, Bytes) {
    let start_key = Bytes::from(key);
    let end_key = Bytes::from({
        let mut data = key.to_vec();
        data.push(0);
        data
    });

    (start_key, end_key)
}

pub fn directory_key_range(dir: &[u8]) -> (Bytes, Bytes) {
    let mut start_key = dir.to_vec();
    if !start_key.ends_with(b"/") {
        start_key.push(b'/');
    }

    let end_key = sstable::table::BytewiseComparator::new().find_short_successor(start_key.clone());

    (start_key.into(), end_key.into())
}
