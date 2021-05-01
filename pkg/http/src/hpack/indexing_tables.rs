// Utilities for working with indexing tables (static + dynamic as one unified entity).

use crate::hpack::static_tables::STATIC_TABLE;
use crate::hpack::dynamic_table::*;
use crate::hpack::header_field::HeaderFieldRef;

/// Looks up a header by its 1-based index as described in RFC 7541: Section 2.3.3
/// An index of 1 is the first value in the static table.
pub fn lookup_header_by_index(mut idx: usize, dynamic_table: &DynamicTable) -> Option<HeaderFieldRef> {
    if idx == 0 {
        return None;
    }

    // Convert to 0 based indexing.
    idx -= 1;

    if let Some(header) = STATIC_TABLE.get(idx) {
        return Some(*header);
    }

    // Align to start of dynamic table.
    idx -= STATIC_TABLE.len();

    dynamic_table.index(idx).map(|header| header.into())
}


pub struct TableSearchResult {
    /// 1-based index of the search result in the concatenated static + dynamic table.
    pub index: usize,
    
    /// Whether or not the value at the index matches the query. Otherwise only the
    /// name matches.
    pub value_matches: bool
}

/// Searches across both the static table and dynamic table in order to find
/// the closet 
pub fn search_for_header(query: HeaderFieldRef, dynamic_table: &DynamicTable) -> Option<TableSearchResult> {
    // Index of the first entry matching only the name of the header.
    // NOTE: We prefer smaller indexes as they will be encoded as fewer bytes.
    let mut first_name_match = None;

    for (i, header) in STATIC_TABLE.iter().enumerate() {
        if header.name != query.name {
            // In the static table, all the entries with the same name are adjacent.
            if first_name_match.is_some() {
                break;
            }

            continue;
        }

        let index = i + 1;
        if header.value == query.value {
            return Some(TableSearchResult {
                index,
                value_matches: true
            })
        } else if first_name_match.is_none() {
            first_name_match = Some(index);
        }
    }

    for i in 0..dynamic_table.len() {
        let header = dynamic_table.index(i).unwrap();

        if header.name != query.name {
            continue;
        }

        let index = STATIC_TABLE.len() + 1 + i;
        if header.value == query.value {
            return Some(TableSearchResult {
                index,
                value_matches: true
            })
        } else if first_name_match.is_none() {
            first_name_match = Some(index);
        }
    }


    if let Some(index) = first_name_match {
        return Some(TableSearchResult {
            index, value_matches: false
        });
    }

    None
}