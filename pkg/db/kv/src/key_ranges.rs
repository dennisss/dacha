use std::collections::BTreeMap;
use std::ops::Bound;

use common::bytes::Bytes;

/// Set of non-overlapping key ranges associated with some data.
pub struct KeyRanges<T> {
    /// Map of the 'start_key' mapped to a (end_key, data) tuple.
    ranges: BTreeMap<Bytes, (Bytes, T)>,
}

/// Used in KeyRanges::iter()
#[derive(Debug, PartialEq)]
pub struct KeyRangesItem<'a, T> {
    pub start_key: &'a Bytes,
    pub end_key: &'a Bytes,
    pub value: &'a T,
}

impl<T: Default + Clone> KeyRanges<T> {
    pub fn new() -> Self {
        Self {
            ranges: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
    }

    /// Mutates all the data associated with all ranges between start_key and
    /// end_key.
    /// - If there doesn't exist a contiguous set of ranges to will the range
    ///   [start_key, end_key), new ranges with T::default() will be created to
    ///   fill the gap and passed to f().
    /// - If [start_key, end_key) only partially overlaps with an existing
    ///   range, the existing range will be split into multiple ranges along the
    ///   start_key or end_key boundaries of the new range.
    ///   - The first split will inherit the data of the existing range
    ///     (pre-splitting).
    ///   - All split ranges after the first will have data initialized with
    ///     T::clone() from the first split's data.
    ///   - All the splits will be passed to f().
    pub fn range<S: Into<Bytes>, E: Into<Bytes>, F: FnMut(&mut T) -> bool>(
        &mut self,
        start_key: S,
        end_key: E,
        mut f: F,
    ) {
        let mut start_key = start_key.into();
        let end_key = end_key.into();

        let lower_bound = {
            if let Some((key, _)) = self
                .ranges
                .range::<[u8], _>((Bound::Unbounded, Bound::Included(&start_key[..])))
                .next_back()
            {
                Bound::Included(key.clone())
            } else if let Some((key, _)) = self.ranges.iter().next() {
                Bound::Included(key.clone())
            } else {
                Bound::Unbounded
            }
        };

        let mut add_ranges = vec![];
        let mut delete_keys = vec![];

        let mut iter = self.ranges.range_mut((lower_bound, Bound::Unbounded));
        while let Some((cur_start_key, (cur_end_key, v))) = iter.next() {
            let cur_end_key: &Bytes = cur_end_key;

            // [cur_start_key] [cur_end_key] [start_key] [end_key]
            if *cur_end_key <= start_key {
                continue;
            }

            // [start_key] [end_key] [cur_start_key] [cur_end_key]
            if *cur_start_key >= end_key {
                break;
            }

            let mut equal_cur = true;

            // Maybe insert an entry before the current entry.
            // [start_key] [cur_start_key] ..
            if start_key < *cur_start_key {
                let new_start_key = start_key.clone();
                let new_end_key = cur_start_key.clone();
                let mut new_value = T::default();

                if f(&mut new_value) {
                    add_ranges.push((new_start_key, (new_end_key, new_value)));
                }

                // Advance start_key.
                start_key = cur_start_key.clone();

                // equal_cur = false;
            } else if start_key > *cur_start_key {
                // Must truncate the current entry at 'start_key' and insert a new entry.
                // [cur_start_key] [start_key] ...

                {
                    delete_keys.push(cur_start_key.clone());

                    let new_start_key = cur_start_key.clone();
                    let new_end_key = start_key.clone();
                    let new_value = v.clone();
                    add_ranges.push((new_start_key, (new_end_key, new_value)));
                }

                {
                    let new_start_key = start_key.clone();
                    let new_end_key = cur_end_key.clone();
                    let mut new_value = v.clone();
                    if f(&mut new_value) {
                        add_ranges.push((new_start_key, (new_end_key, new_value)));
                    }
                }

                equal_cur = false;
            }

            // At this point, we have processed everything up to the start_key and start_key
            // >= cur_start_key.

            if end_key < *cur_end_key {
                // Need to split the current entry into two entries.

                // (start_key -> end_key)
                // (end_key -> cur_key_end)

                // TODO: If we haven't yet mutated the current range, we should mutate it
                // in-place.
                {
                    delete_keys.push(cur_start_key.clone());

                    let new_start_key = start_key.clone();
                    let new_end_key = end_key.clone();
                    let mut new_value = v.clone();
                    if f(&mut new_value) {
                        add_ranges.push((new_start_key, (new_end_key, new_value)));
                    }
                }

                // Create the new entry AFTER the current entry. Note that this will be beyond
                // the range passed in the function arguments so we don't pass it to the user
                // function.
                {
                    let new_start_key = end_key.clone();
                    let new_end_key = cur_end_key.clone();
                    let new_value = v.clone();
                    add_ranges.push((new_start_key, (new_end_key, new_value)));
                }

                equal_cur = false;
            }

            // Simple case of existing range being completely contained by the new range.
            if equal_cur {
                if !f(v) {
                    delete_keys.push(cur_start_key.clone());
                }
            }

            // Advance beyond already processed entries.
            start_key = cur_end_key.clone();
        }

        if start_key < end_key {
            let mut new_value = T::default();
            if f(&mut new_value) {
                add_ranges.push((start_key, (end_key, new_value)));
            }
        }

        for key in delete_keys {
            self.ranges.remove(&key);
        }

        for (key, value) in add_ranges {
            self.ranges.insert(key, value);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = KeyRangesItem<T>> {
        self.ranges
            .iter()
            .map(|(start_key, (end_key, value))| KeyRangesItem {
                start_key,
                end_key,
                value,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_test() {
        let mut r = KeyRanges::<String>::new();

        r.range("a", "c", |s| {
            s.push('1');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[KeyRangesItem {
                start_key: &"a".into(),
                end_key: &"c".into(),
                value: &"1".into()
            }]
        );

        // Completely of an existing range with same start_key.
        r.range("a", "b", |s| {
            s.push('2');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"12".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"1".into()
                }
            ]
        );

        // New non-overlapping range.
        r.range("c", "f", |s| {
            s.push('3');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"12".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"c".into(),
                    end_key: &"f".into(),
                    value: &"3".into()
                }
            ]
        );

        // Partial overlap with an existing range with new ranges AFTER the existing
        // range.
        r.range("e", "l", |s| {
            s.push('4');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"12".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"c".into(),
                    end_key: &"e".into(),
                    value: &"3".into()
                },
                KeyRangesItem {
                    start_key: &"e".into(),
                    end_key: &"f".into(),
                    value: &"34".into()
                },
                KeyRangesItem {
                    start_key: &"f".into(),
                    end_key: &"l".into(),
                    value: &"4".into()
                },
            ]
        );

        // Completely overlapping and split into three segments.
        r.range("j", "k", |s| {
            s.push('5');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"12".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"c".into(),
                    end_key: &"e".into(),
                    value: &"3".into()
                },
                KeyRangesItem {
                    start_key: &"e".into(),
                    end_key: &"f".into(),
                    value: &"34".into()
                },
                KeyRangesItem {
                    start_key: &"f".into(),
                    end_key: &"j".into(),
                    value: &"4".into()
                },
                KeyRangesItem {
                    start_key: &"j".into(),
                    end_key: &"k".into(),
                    value: &"45".into()
                },
                KeyRangesItem {
                    start_key: &"k".into(),
                    end_key: &"l".into(),
                    value: &"4".into()
                },
            ]
        );

        // TODO: Test partial overlap with an existing range with new ranges BEFORE the
        // existing range.

        // TODO: Test inserting a completely overlapping range with same end_key as an
        // existing key.

        for item in r.iter() {
            println!("{:?}", item);
        }
    }
}
