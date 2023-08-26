use common::errors::*;
use crypto::random::{SharedRng, SharedRngExt};
use executor::child_task::ChildTask;
use file::temp::TempDir;
use protobuf::text::ParseTextProto;
use raft::proto::RouteLabel;

use super::client::{MetastoreClient, MetastoreClientInterface};
use crate::meta::test_store::TestMetastore;
use crate::proto::KeyValueEntry;

#[testcase]
async fn basic_pointwise_operations() -> Result<()> {
    let inst = TestMetastore::create().await?;

    let client1 = inst.create_client().await?;

    assert_eq!(client1.get(b"apples").await?, None);
    assert_eq!(client1.get(b"oranges").await?, None);

    client1.put(b"apples", b"one").await?;
    assert_eq!(client1.get(b"apples").await?, Some(b"one".to_vec()));
    assert_eq!(client1.get(b"oranges").await?, None);

    client1.put(b"oranges", b"two").await?;
    assert_eq!(client1.get(b"apples").await?, Some(b"one".to_vec()));
    assert_eq!(client1.get(b"oranges").await?, Some(b"two".to_vec()));

    client1.delete(b"apples").await?;
    assert_eq!(client1.get(b"apples").await?, None);
    assert_eq!(client1.get(b"oranges").await?, Some(b"two".to_vec()));

    // Deleting an already deleted key should be a no-op.
    client1.delete(b"apples").await?;
    client1.delete(b"apples").await?;
    assert_eq!(client1.get(b"apples").await?, None);
    assert_eq!(client1.get(b"oranges").await?, Some(b"two".to_vec()));

    // Create many different generations of "oranges"
    client1.put(b"oranges", b"3").await?;
    client1.put(b"oranges", b"4").await?;
    client1.put(b"oranges", b"5").await?;
    assert_eq!(client1.get(b"oranges").await?, Some(b"5".to_vec()));

    Ok(())
}

#[testcase]
async fn get_range_of_keys() -> Result<()> {
    let inst = TestMetastore::create().await?;

    let client1 = inst.create_client().await?;

    let mut keys = vec![
        "/fruit/apple",
        "/fruit/orange",
        "/fruit/blueberry",
        "/fruitcake/christmas",
        "/vegetable/carrot",
        "/vegetable/lettuce",
    ];

    // Ensure that the keys aren't already in sorted order.
    crypto::random::global_rng().shuffle(&mut keys).await;

    for key in keys {
        client1.put(key.as_bytes(), b"x").await?;
    }

    let mut fruit = client1.get_prefix(b"/fruit/").await?;
    for entry in &mut fruit {
        entry.set_sequence(0u64); // Ignore for comparisons.
    }

    assert_eq!(
        &fruit[..],
        &[
            KeyValueEntry::parse_text(r#"key: "/fruit/apple" value: "x" "#)?,
            KeyValueEntry::parse_text(r#"key: "/fruit/blueberry" value: "x" "#)?,
            KeyValueEntry::parse_text(r#"key: "/fruit/orange" value: "x" "#)?,
        ]
    );

    let mut morefruit = client1.get_prefix(b"/fruit").await?;
    for entry in &mut morefruit {
        entry.set_sequence(0u64); // Ignore for comparisons.
    }

    assert_eq!(
        &morefruit[..],
        &[
            KeyValueEntry::parse_text(r#"key: "/fruit/apple" value: "x" "#)?,
            KeyValueEntry::parse_text(r#"key: "/fruit/blueberry" value: "x" "#)?,
            KeyValueEntry::parse_text(r#"key: "/fruit/orange" value: "x" "#)?,
            KeyValueEntry::parse_text(r#"key: "/fruitcake/christmas" value: "x" "#)?,
        ]
    );

    client1.delete(b"/vegetable/carrot").await?;

    let mut veges = client1.get_prefix(b"/vege").await?;
    for entry in &mut veges {
        entry.set_sequence(0u64); // Ignore for comparisons.
    }

    assert_eq!(
        &veges[..],
        &[KeyValueEntry::parse_text(
            r#"key: "/vegetable/lettuce" value: "x" "#
        )?,]
    );

    Ok(())
}

fn assert_is_failed_txn(result: Result<()>) {
    let error = result.unwrap_err();
    let status = error.downcast_ref::<rpc::Status>().unwrap();
    assert_eq!(status.code(), rpc::StatusCode::FailedPrecondition);
}

#[testcase]
async fn transactions_test() -> Result<()> {
    let inst = TestMetastore::create().await?;

    let client1 = inst.create_client().await?;
    let client2 = inst.create_client().await?;

    client1.put(b"apples", b"1").await?;
    client1.put(b"oranges", b"2").await?;

    let txn2 = client2.new_transaction().await?;
    assert_eq!(txn2.get(b"apples").await?, Some(b"1".to_vec()));

    // Write only visible inside of the transactions.
    txn2.put(b"apples", b"3").await?;
    assert_eq!(client1.get(b"apples").await?, Some(b"1".to_vec()));
    assert_eq!(txn2.get(b"apples").await?, Some(b"3".to_vec()));

    // Make a write that is not visible to the transaction.
    client1.put(b"oranges", b"4").await?;
    assert_eq!(client1.get(b"oranges").await?, Some(b"4".to_vec()));

    // But by reading the key, it should now be in our read set.
    assert_eq!(txn2.get(b"oranges").await?, Some(b"2".to_vec()));

    // So commiting our transaction will fail.
    assert_is_failed_txn(txn2.commit().await);

    // No changes from the commit should be visible.
    assert_eq!(client1.get(b"apples").await?, Some(b"1".to_vec()));
    assert_eq!(client1.get(b"oranges").await?, Some(b"4".to_vec()));
    assert_eq!(client2.get(b"apples").await?, Some(b"1".to_vec()));
    assert_eq!(client2.get(b"oranges").await?, Some(b"4".to_vec()));

    Ok(())
}

async fn transaction_key_range_test() -> Result<()> {
    let inst = TestMetastore::create().await?;

    let client1 = inst.create_client().await?;
    let client2 = inst.create_client().await?;

    let mut keys = vec![
        "/fruit/apple",
        "/fruit/orange",
        "/fruit/blueberry",
        "/vegetable/carrot",
        "/vegetable/lettuce",
    ];
    for key in keys {
        client1.put(key.as_bytes(), b"x").await?;
    }

    {
        let txn1 = client1.new_transaction().await?;
        let txn2 = client2.new_transaction().await?;

        assert_eq!(txn1.get_prefix(b"/fruit/").await?.len(), 3);
        txn1.put(b"/count", b"3").await?;

        assert_eq!(txn2.get_prefix(b"/fruit/").await?.len(), 3);
        txn2.put(b"/count", b"3").await?;

        txn1.commit().await?;
        assert_is_failed_txn(txn2.commit().await);
    }

    {
        let txn1 = client1.new_transaction().await?;
        let txn2 = client2.new_transaction().await?;

        assert_eq!(txn1.get_prefix(b"/fruit/").await?.len(), 3);
        txn1.put(b"/fruit/cherry", b"y").await?;
        txn1.put(b"/count", b"4").await?;

        assert_eq!(txn2.get_prefix(b"/fruit/").await?.len(), 3);
        txn2.put(b"/fruit/melon", b"z").await?;
        txn2.put(b"/count", b"4").await?;

        txn1.commit().await?;
        assert_is_failed_txn(txn2.commit().await);
    }

    // Non-overlapping ranges.
    {
        let txn1 = client1.new_transaction().await?;
        let txn2 = client2.new_transaction().await?;

        assert_eq!(txn1.get_prefix(b"/fruit/").await?.len(), 4);
        txn1.put(b"/fruit/pear", b"y").await?;
        txn1.put(b"/count", b"5").await?;

        assert_eq!(txn2.get_prefix(b"/vegetable/").await?.len(), 2);
        txn2.put(b"/vegetable/arugula", b"z").await?;
        txn2.put(b"/count_v", b"3").await?;

        txn1.commit().await?;
        txn2.commit().await?;
    }

    // TODO: Test when the key ranges don't exactly line up (e.g. only a key keys in
    // each key range overlap).

    Ok(())
}
