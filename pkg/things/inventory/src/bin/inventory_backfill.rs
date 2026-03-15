#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::collections::{HashMap, HashSet};

use common::errors::*;

use inventory::tables::*;
use inventory_proto::inventory::*;
use db_table::*;
use cluster_client::ClusterMetaClient;
use crypto::random::RngExt;

#[derive(Args)]
struct Args {
    #[arg(default = false)]
    write: bool,
}

async fn get_mcmaster_parts() -> Result<Vec<Part>> {

    // Map from mcmaster part number to Part proto.
    let mut part_map = HashMap::<String, Part>::new();

    let cookies = file::read_to_string("/home/dennis/.credentials/mcmaster-cookies.txt").await?.trim().to_string();
    let client = mcmaster::McMasterWebClient::create(cookies).await?;

    let order_ids = client.list_orders().await?;
    // println!("Orders: {:?}", order_ids);

    for order_id in order_ids {
        if order_id == mcmaster::EMPTY_CURRENTORDER {
            continue;
        }

        let order = client.get_order_details(&order_id).await?;

        // println!(
        //     "Order '{}' on {}; Total: {}",
        //     order.inner.Title,
        //     order.inner.PlacedTime,
        //     order.inner.InvoiceTotals.TotalAmtTxt
        // );

        for detail_group in order.inner.DetailGroups {
            for row in detail_group.DtlRows {
                let part_num = row.PartNbr.clone();
                let quantity = mcmaster::raw_quantity(&row)?;

                let part = part_map.entry(part_num.clone()).or_default();
                part.set_name(&row.Title);
                part.source_mut().set_mcmaster_part_number(&part_num);
                
                *part.source_mut().purchased_quantity_mut() += quantity as u64;
                
                // println!("    Line #{}: [Part #{}]: {} [Quantity: {}]", row.LineNbr, row.PartNbr, row.Title, quantity);
            }
        }

        // println!("");
    }

    Ok(part_map.into_values().collect())
}

#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    let client = ClusterMetaClient::create_from_environment().await?;
    let mut rng = crypto::random::clocked_rng();

    let mut existing_part_numbers = HashSet::<String>::default();
    {
        let parts = client.db().list::<PartTable>().await?;
        for part in parts {
            if !part.source().mcmaster_part_number().is_empty() {
                existing_part_numbers.insert(part.source().mcmaster_part_number().to_string());
            }
        }
    }

    let mcmaster_parts = get_mcmaster_parts().await?;

    for mut part in mcmaster_parts {

        if existing_part_numbers.contains(part.source().mcmaster_part_number()) {
            continue;
        }

        println!("NEW PART: {:?}", part);


        if args.write {
            let mut txn = client.db().new_transaction().await?;

            part.set_id(rng.uniform::<u64>());
            txn.put::<PartTable>(&part).await?;

            let mut pack = Pack::default();
            pack.set_id(rng.uniform::<u64>());
            pack.set_part_id(part.id());
            txn.put::<PackTable>(&pack).await?;

            println!("{:?}", part);

            txn.commit().await?;
        }
    }


    Ok(())
}