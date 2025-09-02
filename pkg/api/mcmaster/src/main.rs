#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    let cookies = file::read_to_string("/home/dennis/.credentials/mcmaster-cookies.txt").await?.trim().to_string();
    let client = mcmaster::McMasterWebClient::create(cookies).await?;

    let order_ids = client.list_orders().await?;

    println!("Orders: {:?}", order_ids);

    for order_id in order_ids {
        if order_id == mcmaster::EMPTY_CURRENTORDER {
            continue;
        }

        let order = client.get_order_details(&order_id).await?;

        println!(
            "Order '{}' on {}; Total: {}",
            order.inner.Title,
            order.inner.PlacedTime,
            order.inner.InvoiceTotals.TotalAmtTxt
        );

        for detail_group in order.inner.DetailGroups {
            for row in detail_group.DtlRows {
                let quantity = mcmaster::raw_quantity(&row)?;
                println!("    Line #{}: [Part #{}]: {} [Quantity: {}]", row.LineNbr, row.PartNbr, row.Title, quantity);
            }
        }

        println!("");
    }

    Ok(())
}