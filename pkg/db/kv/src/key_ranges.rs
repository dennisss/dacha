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

    // TODO: Add unit tests for this.
    pub fn range<F: FnMut(&T)>(&self, start_key: &[u8], end_key: &[u8], mut f: F) {
        if start_key >= end_key {
            return;
        }

        // TODO: Dedup with range_mut
        let lower_bound: Bound<&[u8]> = {
            if let Some((key, _)) = self
                .ranges
                .range::<[u8], _>((Bound::Unbounded, Bound::Included(start_key)))
                .next_back()
            {
                Bound::Included(&key)
            } else if let Some((key, _)) = self.ranges.iter().next() {
                Bound::Included(&key)
            } else {
                Bound::Unbounded
            }
        };

        let mut iter = self.ranges.range::<[u8], (Bound<&[u8]>, Bound<&[u8]>)>((lower_bound, Bound::Unbounded));
        while let Some((cur_start_key, (cur_end_key, v))) = iter.next() {
            // If true, the current entry was marked for deletion from the map.
            let mut cur_virtual = false;

            // [cur_start_key] [cur_end_key] [start_key] [end_key]
            if *cur_end_key <= start_key {
                continue;
            }

            // [start_key] [end_key] [cur_start_key] [cur_end_key]
            if *cur_start_key >= end_key {
                break;
            }

            f(v);
        }
    }

    /// Mutates all the data associated with all ranges between start_key and
    /// end_key.
    /// - If there doesn't exist a contiguous set of ranges in the range
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
    /// 
    /// Implementing 'f()':
    /// - Will be given the old values of T that existed in the range
    ///   (or T::default() for unoccupied ranges).
    /// - Should return true if the data in T is non-empty (should be kept in
    ///   this data structure)
    ///
    /// TODO: We need to implement some merging mechanism to combine consecutive
    /// range values that end up being identical. This is mainly a problem for the
    /// 'watchers' feature were long lived ranges can be indefinitely sharded
    /// by many short lived single row watchers. 
    pub fn range_mut<S: Into<Bytes>, E: Into<Bytes>, F: FnMut(&mut T) -> bool>(
        &mut self,
        start_key: S,
        end_key: E,
        mut f: F,
    ) {
        let mut start_key = start_key.into();
        let end_key = end_key.into();

        if start_key >= end_key {
            return;
        }

        // TODO: Can this be merged with the construction of 'iter'.
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
            // If true, the current entry was marked for deletion from the map.
            let mut cur_virtual = false;

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
            } else if start_key > *cur_start_key {
                // Must truncate the current entry to end at 'start_key'.
                // [cur_start_key] [start_key] ...

                {
                    delete_keys.push(cur_start_key.clone());

                    let new_start_key = cur_start_key.clone();
                    let new_end_key = start_key.clone();
                    let new_value = v.clone();
                    add_ranges.push((new_start_key, (new_end_key, new_value)));
                }

                // We just deleted the current entry.
                cur_virtual = true;
            }

            // At this point, we can assume that start_key == cur_start_key

            if end_key < *cur_end_key {
                // Need to split the current entry into two entries.

                // (start_key -> end_key)
                // (end_key -> cur_key_end)

                // TODO: If we haven't yet mutated the current range, we should mutate it
                // in-place.
                {
                    if !cur_virtual {
                        delete_keys.push(start_key.clone());
                    }

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
            } else {
                // Will be in one of the following two cases. In both cases we don't need to
                // do any splitting:
                // 
                // - 'end_key == cur_end_key'
                // - 'end_key > cur_end_key'

                if f(v) {
                    if cur_virtual {
                        let new_start_key = start_key.clone();
                        let new_end_key = cur_end_key.clone();
                        let new_value = v.clone();
                        add_ranges.push((new_start_key, (new_end_key, new_value)));
                    }
                } else {
                    if !cur_virtual {
                        delete_keys.push(cur_start_key.clone());
                    }
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
            assert!(self.ranges.remove(&key).is_some());
        }

        for (key, value) in add_ranges {
            assert!(self.ranges.insert(key, value).is_none());
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
    fn test_bounds() {
        let mut r = KeyRanges::<String>::new();

        r.range_mut("a", "z", |s| {
            s.push('1');
            true
        });

        r.range_mut("b", "y", |s| {
            s.push('2');
            true
        });

        r.range_mut("b", "y", |s| {
            s.push('3');
            true
        });

        r.range_mut("b", "y", |s| {
            s.push('4');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"y".into(),
                    value: &"1234".into()
                },
                KeyRangesItem {
                    start_key: &"y".into(),
                    end_key: &"z".into(),
                    value: &"1".into()
                },    
            ]
        );

        r.range_mut("c", "y", |s| {
            s.push('5');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"1234".into()
                },
                KeyRangesItem {
                    start_key: &"c".into(),
                    end_key: &"y".into(),
                    value: &"12345".into()
                },
                KeyRangesItem {
                    start_key: &"y".into(),
                    end_key: &"z".into(),
                    value: &"1".into()
                },    
            ]
        );
    }

    #[test]
    fn test_in_between() {
        let mut r = KeyRanges::<String>::new();

        r.range_mut("a", "z", |s| {
            s.push('1');
            true
        });

        r.range_mut("c", "x", |s| {
            s.push('2');
            true
        });

        r.range_mut("b", "y", |s| {
            s.push('3');
            true
        });

        assert_eq!(
            &r.iter().collect::<Vec<_>>(),
            &[
                KeyRangesItem {
                    start_key: &"a".into(),
                    end_key: &"b".into(),
                    value: &"1".into()
                },
                KeyRangesItem {
                    start_key: &"b".into(),
                    end_key: &"c".into(),
                    value: &"13".into()
                },
                KeyRangesItem {
                    start_key: &"c".into(),
                    end_key: &"x".into(),
                    value: &"123".into()
                },
                KeyRangesItem {
                    start_key: &"x".into(),
                    end_key: &"y".into(),
                    value: &"13".into()
                },
                KeyRangesItem {
                    start_key: &"y".into(),
                    end_key: &"z".into(),
                    value: &"1".into()
                },
            ]
        );




    }


    #[test]
    fn overlaps_test() {
        let mut r = KeyRanges::<String>::new();

        r.range_mut("a", "c", |s| {
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
        r.range_mut("a", "b", |s| {
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
        r.range_mut("c", "f", |s| {
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
        r.range_mut("e", "l", |s| {
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
        r.range_mut("j", "k", |s| {
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
