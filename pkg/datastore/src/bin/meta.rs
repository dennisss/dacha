#[macro_use]
extern crate macros;

use common::args::parse_args;
use common::errors::*;
use rpc_util::NamedPortArg;

use crypto::random::{Rng, SharedRng};

// TODO: Test the implementation by repeatably using a transaction to increment
// a counter.
// - Then we can verify that all versions of the counter key are monotonic.

use datastore_meta_client::*;

async fn increment_counter(txn: &dyn MetastoreClientInterface) -> Result<()> {
    let mut current_num = 0;
    if let Some(value) = txn.get(b"/counter").await? {
        current_num = std::str::from_utf8(&value)?.parse::<usize>()?;
    }

    println!("INITIAL NUM: {}", current_num);

    current_num += 1;

    txn.put(b"/counter", current_num.to_string().as_bytes())
        .await?;
    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let client = MetastoreClient::create(&[], &[], None).await?;

    {
        let mut data = vec![0; 1024 * 1024];

        let rng = crypto::random::global_rng();

        for i in 0..100 {
            println!("{}", i);
            rng.generate_bytes(&mut data).await;
            client.put(format!("key{}", i).as_bytes(), &data).await?;
        }

        return Ok(());
    }

    {
        let txn1 = client.new_transaction().await?;
        let txn2 = client.new_transaction().await?;

        increment_counter(&txn1).await?;
        increment_counter(&txn2).await?;

        txn1.commit().await?;

        println!("COMMIT TXN 2");
        txn2.commit().await?; // < This must fail
    }

    let mut txn = client.new_transaction().await?;
    txn.get(b"/hello").await?;
    txn.put(b"/first", b"hello").await?;
    txn.put(b"/second", b"melon").await?;

    txn.commit().await?;

    let items = client.get_prefix(b"/").await?;
    for item in items {
        println!("{:?}", item);
    }

    Ok(())
}
