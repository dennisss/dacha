use std::cmp::Ordering;
use std::sync::Arc;

use common::errors::*;

use crate::table::comparator::KeyComparator;
use crate::table::filter_policy::FilterPolicy;

const MAX_SEQUENCE: u64 = (1 << 56) - 1;

// TODO: Switch to the last value in the ValueType.
const VALUE_FOR_SEEK: ValueType = ValueType::MaxValueType; // Not really meaninful right now.

enum_def!(ValueType u8 =>
    Deletion = 0,
    Value = 1,
    Merge = 2,
    LogData = 3,
    ColumnFamilyDeletion = 4,
    ColumnFamilyValue = 5,
    ColumnFamilyMerge = 6,
    SingleDeletion = 7,
    ColumnFamilySingleDeletion = 8,
    BeginPrepareXID = 0x09,
    EndPrepareXID = 0x0A,
    CommitXID = 0x0B,
    RollbackXID = 0x0C,
    Noop = 0x0D,
    ColumnFamilyRangeDeletion = 0x0E,
    RangeDeletion = 0x0F,
    ColumnFamilyBlobIndex = 0x10,
    BlobIndex = 0x11,
    BeginPersistedPrepareXID = 0x12,
    BeginUnprepareXID = 0x13,

    MaxValueType = 0xff
);

#[derive(Debug)]
pub struct InternalKey<'a> {
    pub user_key: &'a [u8],
    pub sequence: u64,
    pub typ: ValueType,
}

impl<'a> InternalKey<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self> {
        min_size!(input, 8);
        let (user_key, rest) = input.split_at(input.len() - 8);
        let num = u64::from_le_bytes(*array_ref![rest, 0, 8]);
        let typ = ValueType::from_value((num & 0xff) as u8)?;
        let sequence = num >> 8;

        Ok(Self {
            user_key,
            sequence,
            typ,
        })
    }

    pub fn user_key(input: &[u8]) -> &[u8] {
        assert!(input.len() >= 8);
        &input[..(input.len() - 8)]
    }

    /// Creates an internal key that is positioned immediately before all keys
    /// with the given user_key.
    ///
    /// This is mainly used for seeking to the position immediately before a
    /// user key.
    pub fn before(user_key: &'a [u8]) -> Self {
        Self::before_with_sequence(user_key, MAX_SEQUENCE)
    }

    pub fn before_with_sequence(user_key: &'a [u8], sequence: u64) -> Self {
        Self {
            user_key,
            sequence,
            typ: VALUE_FOR_SEEK,
        }
    }

    // TODO: Usually we don't need to serialize, we just need to get a user_key
    // from the key or append the sequence and type to a user key vector.
    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.reserve(self.user_key.len() + 8);
        out.extend_from_slice(&self.user_key);
        let num: u64 = (self.sequence << 8) | (self.typ as u64);
        out.extend_from_slice(&num.to_le_bytes());
    }

    pub fn serialized(&self) -> Vec<u8> {
        let mut out = vec![];
        self.serialize(&mut out);
        out
    }
}

pub struct InternalKeyComparator {
    user_key_comparator: Arc<dyn KeyComparator>,
}

impl InternalKeyComparator {
    pub fn wrap(user_key_comparator: Arc<dyn KeyComparator>) -> Arc<Self> {
        Arc::new(Self {
            user_key_comparator,
        })
    }
}

impl KeyComparator for InternalKeyComparator {
    fn name(&self) -> &'static str {
        self.user_key_comparator.name()
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        let a_ik = InternalKey::parse(a).unwrap();
        let b_ik = InternalKey::parse(b).unwrap();

        // TODO: Consider the type as well?
        // Just compare last 8 bytes as a little endian value.
        match self
            .user_key_comparator
            .compare(a_ik.user_key, b_ik.user_key)
        {
            // Decreasing sequence order
            // TODO: If they are equal (which they should never be, compare be
            // decreasing type.
            Ordering::Equal => b_ik.sequence.cmp(&a_ik.sequence),
            o @ _ => o,
        }
    }

    fn find_shortest_separator(&self, mut start: Vec<u8>, limit: &[u8]) -> Vec<u8> {
        // NOTE: This is different than how LevelDB does it
        let end = u64::from_ne_bytes(*array_ref![start, start.len() - 8, 8]);
        start.truncate(start.len() - 8);
        start = self
            .user_key_comparator
            .find_shortest_separator(start, InternalKey::user_key(limit));
        start.extend_from_slice(&end.to_ne_bytes());
        start
    }

    // TODO: Eventually support removing the 8 bytes as we don't really need
    // the return value to be a legal key.
    fn find_short_successor(&self, mut key: Vec<u8>) -> Vec<u8> {
        // NOTE: This is different than how LevelDB does it, but should still
        // enforce the >= policy with fewer allocations.
        let end = u64::from_ne_bytes(*array_ref![key, key.len() - 8, 8]);
        key.truncate(key.len() - 8);
        key = self.user_key_comparator.find_short_successor(key);
        key.extend_from_slice(&end.to_ne_bytes()); // &[0xffu8; 8]);
        key
    }
}

pub struct InternalKeyFilterPolicy {
    user_key_filter_policy: Arc<dyn FilterPolicy>,
}

impl InternalKeyFilterPolicy {
    pub fn wrap(user_key_filter_policy: Arc<dyn FilterPolicy>) -> Arc<dyn FilterPolicy> {
        Arc::new(Self {
            user_key_filter_policy,
        })
    }
}

impl FilterPolicy for InternalKeyFilterPolicy {
    fn name(&self) -> &'static str {
        self.user_key_filter_policy.name()
    }

    fn create(&self, mut keys: Vec<&[u8]>, out: &mut Vec<u8>) {
        for k in keys.iter_mut() {
            *k = InternalKey::user_key(*k);
        }

        self.user_key_filter_policy.create(keys, out);
    }

    fn key_may_match(&self, key: &[u8], filter: &[u8]) -> bool {
        self.user_key_filter_policy
            .key_may_match(InternalKey::user_key(key), filter)
    }
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, sync::Arc};

    use crate::table::{comparator::BytewiseComparator, KeyComparator};

    use super::{InternalKey, InternalKeyComparator, ValueType};

    #[test]
    fn internal_key_serialize_test() {
        let ikey = InternalKey {
            user_key: &[0xaa, 0xbb, 0xcc],
            sequence: 0x5524,
            typ: ValueType::Value,
        };

        assert_eq!(
            &ikey.serialized(),
            &[0xaa, 0xbb, 0xcc, 0x01, 0x24, 0x55, 0, 0, 0, 0, 0]
        );

        let ikey2 =
            InternalKey::parse(&[0xaa, 0xbb, 0xcc, 0x01, 0x24, 0x55, 0, 0, 0, 0, 0]).unwrap();

        assert_eq!(ikey2.user_key, ikey.user_key);
        assert_eq!(ikey2.sequence, ikey.sequence);
        assert_eq!(ikey2.typ, ikey.typ);
    }

    #[test]
    fn internal_key_compare_test() {
        let cmp = InternalKeyComparator::wrap(Arc::new(BytewiseComparator::new()));

        let ikey1 = InternalKey {
            user_key: &[1],
            sequence: 100,
            typ: ValueType::Value,
        }
        .serialized();

        let ikey2 = InternalKey {
            user_key: &[1],
            sequence: 200,
            typ: ValueType::Value,
        }
        .serialized();

        let ikey3 = InternalKey {
            user_key: &[2],
            sequence: 150,
            typ: ValueType::Value,
        }
        .serialized();

        assert_eq!(cmp.compare(&ikey1, &ikey2), Ordering::Greater);
        assert_eq!(cmp.compare(&ikey1, &ikey1), Ordering::Equal);
        assert_eq!(cmp.compare(&ikey2, &ikey1), Ordering::Less);

        assert_eq!(cmp.compare(&ikey1, &ikey3), Ordering::Less);
        assert_eq!(cmp.compare(&ikey2, &ikey3), Ordering::Less);
    }
}
