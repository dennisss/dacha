use common::args::parse_args;
use common::async_std::path::PathBuf;
use common::async_std::task::block_on;
use common::errors::*;
use rpc_util::NamedPortArg;

// TODO: Test the implementation by repeatably using a transaction to increment
// a counter.
// - Then we can verify that all versions of the counter key are monotonic.

use datastore::meta::client::*;

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

async fn run() -> Result<()> {
    let client = MetastoreClient::create(&[]).await?;

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

fn main() -> Result<()> {
    // let args = parse_args::<Args>()?;
    block_on(run())
}
