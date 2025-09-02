use common::errors::*;

use crate::web_client::DetailRow;

pub fn raw_quantity(row: &DetailRow) -> Result<usize> {
    let quantity = row.Quantity.parse::<usize>()?;

    let per_quantity = 
        if let Some(suffix) = row.QuantityUnit.strip_prefix("Packs of ") {
            suffix.trim().parse::<usize>()?
        } else if let Some(suffix) = row.QuantityUnit.strip_prefix("Pack of ") {
            suffix.trim().parse::<usize>()?
        } else if row.QuantityUnit == "Each" {
            1
        } else {
            return Err(format_err!("Unknown quantity format for '{}'", row.QuantityUnit));
        };

    Ok(quantity * per_quantity)
}