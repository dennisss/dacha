use std::sync::Arc;

use common::errors::*;

use crate::db::version::Version;
use crate::iterable::{Iterable, KeyValueEntry};
use crate::table::comparator::KeyComparator;
use crate::table::table::{DataBlockCache, SSTableIterator};
use crate::EmbeddedDBOptions;

/// NOTE: This is for level 1+.
pub struct LevelIterator {
    version: Arc<Version>,
    options: Arc<EmbeddedDBOptions>,

    // ^ All the above is in the snapshot.
    level_index: usize,
    next_table_index: usize,
    current_table_iterator: Option<SSTableIterator>,
}

impl LevelIterator {
    pub fn new(version: Arc<Version>, level_index: usize, options: Arc<EmbeddedDBOptions>) -> Self {
        Self {
            version,
            options,
            level_index,
            next_table_index: 0,
            current_table_iterator: None,
        }
    }
}

#[async_trait]
impl Iterable for LevelIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        let tables = &self.version.levels[self.level_index].tables;

        loop {
            let mut iter = match self.current_table_iterator.take() {
                Some(iter) => iter,
                None => {
                    if self.next_table_index >= tables.len() {
                        return Ok(None);
                    }

                    let table = tables[self.next_table_index].table.lock().await;

                    let iter = table.as_ref().unwrap().iter(&self.options.block_cache);
                    self.next_table_index += 1;

                    iter
                }
            };

            let entry = iter.next().await?;
            if let Some(entry) = entry {
                self.current_table_iterator = Some(iter);
                return Ok(Some(entry));
            }
        }
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        let tables = &self.version.levels[self.level_index].tables[..];
        let key_comparator = self.options.table_options.comparator.as_ref();

        let table_idx = common::algorithms::lower_bound_by(tables, key, |table, key| {
            key_comparator
                .compare(&table.entry.largest_key, key)
                .is_ge()
        });

        if let Some(idx) = table_idx {
            let table = tables[idx].table.lock().await;
            let mut iter = table.as_ref().unwrap().iter(&self.options.block_cache);
            iter.seek(key).await?;

            self.next_table_index = idx + 1;
            self.current_table_iterator = Some(iter);

            return Ok(());
        }

        self.next_table_index = tables.len();
        self.current_table_iterator = None;
        Ok(())
    }
}
