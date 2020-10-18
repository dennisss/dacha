use std::cmp::Ordering;

pub trait Comparator {
    fn name(&self) -> &'static str;

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering;

    /// Finds the shortest key such that start <= key < limit.
    fn find_shortest_separator(&self, start: Vec<u8>, limit: &[u8]) -> Vec<u8>;

    /// Finds the shortest key >= the given key.
    fn find_short_successor(&self, key: Vec<u8>) -> Vec<u8>;
}

pub struct DummyComparator {}

impl DummyComparator {
    pub const fn new() -> Self {
        Self {}
    }
}

impl Comparator for DummyComparator {
    fn name(&self) -> &'static str {
        unimplemented!("")
    }
    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        unimplemented!("")
    }
    fn find_shortest_separator(&self, start: Vec<u8>, limit: &[u8]) -> Vec<u8> {
        unimplemented!("")
    }
    fn find_short_successor(&self, key: Vec<u8>) -> Vec<u8> {
        unimplemented!("")
    }
}

pub struct BytewiseComparator {}

impl BytewiseComparator {
    pub fn new() -> Self {
        Self {}
    }
}

impl Comparator for BytewiseComparator {
    fn name(&self) -> &'static str {
        "leveldb.BytewiseComparator"
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        a.cmp(b)
    }

    fn find_shortest_separator(&self, mut start: Vec<u8>, limit: &[u8]) -> Vec<u8> {
        debug_assert_eq!(self.compare(&start, limit), Ordering::Less);

        // Find common prefix.
        let min_length = std::cmp::min(start.len(), limit.len());
        let mut diff_index = 0;
        while diff_index < min_length && start[diff_index] == limit[diff_index] {
            diff_index += 1;
        }

        if diff_index >= min_length {
            // In this case diff_index == min_length, thus start is a prefix of
            // limit, so no shorter key exists.
            return start;
        }

        for i in diff_index..min_length {
            if start[i] < 0xff && start[i] + 1 < limit[i] {
                start[i] += 1;
                start.truncate(i + 1);
                break;
            }
        }

        debug_assert_eq!(self.compare(&start, limit), Ordering::Less);
        start
    }

    fn find_short_successor(&self, mut key: Vec<u8>) -> Vec<u8> {
        for i in 0..key.len() {
            if key[i] != 0xff {
                key[i] += 1;
                key.truncate(i + 1);
                break;
            }
        }

        key
    }
}
