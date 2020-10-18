use crate::comparator::*;
use crate::comparator_context::ComparatorContext;
use crate::internal_key::*;
use crate::record_log::RecordLog;
use crate::table_builder::*;
use crate::write_batch::Write::Value;
use crate::write_batch::*;
use common::async_std::path::Path;
use common::errors::*;
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

// TODO: Refactor to store InternalKey in parsed form to avoid parsing as part
// of the lookups.
struct MemTableKey<'a> {
    key: Cow<'a, [u8]>,
}

impl<'a> MemTableKey<'a> {
    fn new(key: Vec<u8>) -> Self {
        Self {
            key: Cow::Owned(key),
        }
    }
    fn view(key: &'a [u8]) -> Self {
        Self {
            key: Cow::Borrowed(key),
        }
    }
}

impl Ord for MemTableKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        ComparatorContext::<()>::comparator().compare(&self.key, &other.key)
    }
}
impl PartialOrd for MemTableKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for MemTableKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for MemTableKey<'_> {}

/*
    Key operations:
    - Given an initial internal key, and a seek direction, look up

    - Ideally use
*/

// TODO: To deal with snapshots, we need to store all entries ordered by
// sequence which aren't on disk already. (but can be GC'ed when initializing
// the table from a log)
pub struct MemTable {
    table: ComparatorContext<BTreeMap<MemTableKey<'static>, Vec<u8>>>,
}

impl MemTable {
    pub fn new(comparator: Arc<dyn Comparator>) -> Self {
        Self {
            table: ComparatorContext::new(BTreeMap::new(), comparator),
        }
    }

    //	pub fn get<'a>(&'a self, key: &'a [u8]) -> Option<TableValue<'a>> {
    //		let entry = match self.table.guard().inner().get(
    //			&MemTableKey::view(key)) {
    //			Some(e) => e,
    //			None => { return None; }
    //		};
    //
    //		if entry.typ == ValueType::Deletion {
    //			return Some(TableValue { sequence: entry.sequence, value: None });
    //		}
    //
    //		Some(TableValue { sequence: entry.sequence, value: Some(&entry.value) })
    //	}

    /// Creates an iterator over the memtable starting at the given key.
    pub fn range_from<'a>(&'a self, key: &'a [u8]) -> impl Iterator<Item = (&'a [u8], &'a [u8])> {
        self.table
            .guard()
            .inner()
            .range((Bound::Included(MemTableKey::view(key)), Bound::Unbounded))
            .map(|(k, v)| (k.key.as_ref(), v.as_ref()))
    }

    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.table.guard_mut().insert(MemTableKey::new(key), value);
    }

    /// Writes WriteBatches from the given log file and applies their effects
    /// to the current table.
    pub async fn apply_log(&mut self, log: &mut RecordLog) -> Result<()> {
        while let Some(record) = log.read().await? {
            let (batch, rest) = WriteBatch::parse(&record)?;
            if rest.len() != 0 {
                return Err(err_msg("Extra data after write batch"));
            }

            for w in &batch.writes {
                match w {
                    Write::Value { key, value } => {
                        let ikey = InternalKey {
                            user_key: key,
                            typ: ValueType::Value,
                            sequence: batch.sequence,
                        }
                        .serialized();

                        self.insert(ikey, value.to_vec());
                    }
                    Write::Deletion { key } => {
                        let ikey = InternalKey {
                            user_key: key,
                            typ: ValueType::Deletion,
                            sequence: batch.sequence,
                        }
                        .serialized();

                        self.insert(ikey, vec![]);
                    }
                }
            }
        }

        Ok(())
    }

    /// TODO: Must consider the smallest snapshot sequence
    pub async fn write_table(
        &self,
        path: &Path,
        table_options: SSTableBuilderOptions,
    ) -> Result<()> {
        let mut table_builder = SSTableBuilder::open(path, table_options).await?;
        for (key, value) in &*self.table.guard() {
            let ik = InternalKey::parse(&key.key).unwrap();
            if ik.typ != ValueType::Deletion {
                // TODO: Internalize this cloning?
                table_builder.add(key.key.to_vec(), value.to_vec()).await?;
            }
        }

        table_builder.finish().await?;

        Ok(())
    }
}
//
//pub struct TableValue<'a> {
//	pub sequence: u64,
//	/// If none, then this value was deleted
//	pub value: Option<&'a [u8]>
//}

//pub struct MemTableIterator<'a> {
//	current: (&'a [u8], &'a [u8])
//}

/*
    What can be done while iterating:
    -
*/
