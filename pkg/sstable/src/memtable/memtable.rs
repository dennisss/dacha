use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use common::async_std::path::Path;
use common::errors::*;

use crate::internal_key::*;
use crate::memtable::vec::*;
use crate::record_log::RecordReader;
use crate::table::comparator::*;
use crate::table::table_builder::*;
use crate::write_batch::Write::Value;
use crate::write_batch::*;

/*
Internal table implementation needs to support:
- Iteration (starting at a given key or the beginning)
- Insertion

*/

/*
    Key operations:
    - Given an initial internal key, and a seek direction, look up

    - Ideally use
*/

// TODO: To deal with snapshots, we need to store all entries ordered by
// sequence which aren't on disk already. (but can be GC'ed when initializing
// the table from a log)
pub struct MemTable {
    table: VecMemTable,
}

impl MemTable {
    pub fn new(comparator: Arc<dyn Comparator>) -> Self {
        Self {
            table: VecMemTable::new(comparator),
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
    pub fn range_from(&self, key: &[u8]) -> VecMemTableIterator {
        let mut iter = self.table.iter();
        iter.seek(key);
        iter
    }

    // TODO: Change to taking references as arguments as we eventually want to copy
    // the data into the memtable's arena.
    pub async fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.table.insert(key, value).await;
    }

    /// Writes WriteBatches from the given log file and applies their effects
    /// to the current table.
    pub async fn apply_log(&mut self, log: &mut RecordReader) -> Result<()> {
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

                        self.insert(ikey, value.to_vec()).await;
                    }
                    Write::Deletion { key } => {
                        let ikey = InternalKey {
                            user_key: key,
                            typ: ValueType::Deletion,
                            sequence: batch.sequence,
                        }
                        .serialized();

                        self.insert(ikey, vec![]).await;
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
        let mut iter = self.table.iter();
        while let Some(entry) = iter.next().await {
            let ik = InternalKey::parse(&entry.key).unwrap();
            if ik.typ != ValueType::Deletion {
                // TODO: Internalize this cloning?
                table_builder.add(entry.key.to_vec(), &entry.value).await?;
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
