use std::collections::BTreeMap;

use common::errors::*;

use crate::{
    sql::{CreateIndexStatement, CreateTableStatement, DdlStatement},
    SpannerDatabaseAdminClient, SpannerDatabaseClientOptions,
};

/// Applies DDL changes to a database to align it with an intended schema (using
/// a diff against the current state of the database to figure out what to do).
///
/// Note that the user is responsible for ensuring that only one pusher exists
/// per database.
pub struct SpannerDatabaseSchemaPusher {
    client: SpannerDatabaseAdminClient,
}

#[derive(Debug)]
pub struct SpannerDatabaseDiff {
    statements: Vec<String>,
}

impl SpannerDatabaseSchemaPusher {
    pub async fn create(options: SpannerDatabaseClientOptions) -> Result<Self> {
        Ok(Self {
            client: SpannerDatabaseAdminClient::create(options).await?,
        })
    }

    /// NOTE: All assets related to a single table must be given at the same
    /// time
    pub async fn diff(&self, target_statements: &[DdlStatement]) -> Result<SpannerDatabaseDiff> {
        // Wait for any ongoing schema changes to finish being applied.
        loop {
            let pending_ops = self.client.list_pending_operations().await?;
            if pending_ops.operations().is_empty() {
                break;
            }

            // TODO: Only count typeurl:
            // "type.googleapis.com/google.spanner.admin.database.v1.
            // UpdateDatabaseDdlMetadata" operations?

            self.client
                .wait_for_operation(pending_ops.operations()[0].name())
                .await?;
        }

        let mut diff = SpannerDatabaseDiff { statements: vec![] };

        let target_schema = Schema::create(target_statements)?;

        let current_statements = {
            let mut out = vec![];
            for s in self.client.get_ddl().await?.statements() {
                out.push(DdlStatement::parse(s)?);
            }
            out
        };

        let current_schema = Schema::create(&current_statements)?;

        for target_table in target_schema.tables.values().cloned() {
            if let Some(current_table) = current_schema.tables.get(target_table.table_name.as_str())
            {
                // Alter table.
                if let Some(s) = self.diff_table(*current_table, target_table)? {
                    diff.statements.push(s);
                }
            } else {
                // Create new table.
                diff.statements.push(target_table.to_string());
            }
        }

        for current_index in current_schema.indices.values().cloned() {
            // Ignore tables for which we haven't been given any schemas.
            if !target_schema
                .tables
                .contains_key(current_index.table_name.as_str())
            {
                continue;
            }

            if !target_schema
                .indices
                .contains_key(&current_index.index_name.as_str())
            {
                diff.statements
                    .push(format!("DROP INDEX {}", current_index.index_name));
            }
        }

        for target_index in target_schema.indices.values().cloned() {
            if let Some(current_index) =
                current_schema.indices.get(target_index.index_name.as_str())
            {
                if *current_index != target_index {
                    return Err(err_msg("Can't modify an existing index"));
                }
            } else {
                // Create new
                diff.statements.push(target_index.to_string());
            }
        }

        // TODO: Indexes must be dropped before fields they reference are
        // dropped.

        Ok(diff)
    }

    fn diff_table(
        &self,
        current_table: &CreateTableStatement,
        target_table: &CreateTableStatement,
    ) -> Result<Option<String>> {
        let mut current_columns = BTreeMap::new();
        for col in &current_table.columns {
            if current_columns
                .insert(col.column_name.as_str(), col)
                .is_some()
            {
                return Err(err_msg("Duplicate column"));
            }
        }

        let mut target_columns = BTreeMap::new();
        for col in &target_table.columns {
            if target_columns
                .insert(col.column_name.as_str(), col)
                .is_some()
            {
                return Err(err_msg("Duplicate column"));
            }
        }

        let mut diff = String::new();

        for current_column in current_columns.values().cloned() {
            if let Some(target_column) = target_columns.get(current_column.column_name.as_str()) {
                if current_column != *target_column {
                    return Err(format_err!(
                        "Can't modify an existing column: {:?} vs {:?}",
                        current_column,
                        target_column
                    ));
                }
            } else {
                diff.push_str(&format!(" ADD COLUMN {}", current_column.to_string()));
            }
        }

        for target_column in target_columns.values().cloned() {
            if !current_columns.contains_key(target_column.column_name.as_str()) {
                diff.push_str(&format!(" DROP COLUMN {}", target_column.column_name));
            }
        }

        // TODO: Instead clear fields as we go and diff the entire struct to ensure no
        // differences remain after supported diffs are handled.
        if current_table.primary_key != target_table.primary_key {
            return Err(err_msg("Primary key of table changed"));
        }

        Ok(if diff.is_empty() {
            None
        } else {
            Some(format!("ALTER TABLE {} {}", current_table.table_name, diff))
        })
    }

    /// NOTE: This should be called shortly after diff().
    pub async fn push(&self, diff: SpannerDatabaseDiff) -> Result<()> {
        if diff.statements.is_empty() {
            return Ok(());
        }

        self.client.update_ddl(&diff.statements).await?;

        Ok(())
    }
}

struct Schema<'a> {
    // Btreemaps used here for determinism
    tables: BTreeMap<&'a str, &'a CreateTableStatement>,
    indices: BTreeMap<&'a str, &'a CreateIndexStatement>,
}

impl<'a> Schema<'a> {
    fn create(statements: &'a [DdlStatement]) -> Result<Self> {
        let mut tables = BTreeMap::new();
        let mut indices = BTreeMap::new();

        for statement in statements {
            match statement {
                DdlStatement::CreateTable(v) => {
                    if tables.insert(v.table_name.as_str(), v).is_some() {
                        return Err(err_msg("Duplicate table"));
                    }
                }
                DdlStatement::CreateIndex(v) => {
                    // Tables need to be created before the indexes.
                    if indices.insert(v.index_name.as_str(), v).is_some() {
                        return Err(err_msg("Duplicate index"));
                    }
                }
            }
        }

        Ok(Self { tables, indices })
    }
}
