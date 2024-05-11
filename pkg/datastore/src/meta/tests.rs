use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use crypto::random::{SharedRng, SharedRngExt};
use datastore_meta_client::{MetastoreClient, MetastoreClientInterface};
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor_multitask::ServiceResource;
use file::temp::TempDir;
use protobuf::text::ParseTextProto;
use raft::proto::{Configuration_ServerRole, RouteLabel, ServerId, Status};
use rpc_util::AddProfilingEndpoints;

use crate::meta::test_store::TestMetastore;
use crate::proto::KeyValueEntry;

use super::TestMetastoreCluster;

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
    assert_eq!(status.code(), rpc::StatusCode::Aborted);
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

#[testcase]
async fn multi_node_test() -> Result<()> {
    /*
    TODO: Bigger E2E test:

    - Start one node
    - Write a bunch of keys (few enough that they are still all in the log)
    - Stop the server and restart to verify that it can recover from just the log with no sstables.
    - Add a second node (recovering just from log)
        - Leveraging the ASPIRING mechanism
        - Wait for the node to become a member
    - Write many more keys (get the log to be partially discarded)
    - Stop and restart both nodes to verify that we can restore from local snapshot on server startup
    - Add a third node
        - Should restore from a snapshot
        - Wait for the node to become a member
    - Simulate the second node being disconnected from the network
    - Write many more keys and get some of the new keys to be discarded from the log
    - Re-connect the second node
        - Expect it to recover via a snapshot.
    - Gracefully shutdown server 1 (the leader)
        - Expect timeout now to make another server the leader quickly
    - Bring back up server 1
    - Kill/restart the current leader
        - Verify that client operations can auto-retry during the downtime

    TODO: Need to test waterline compaction


    TODO: Periodically verify that all the state machines across the nodes are identical by performing follower reads on all nodes (will require the followers to give feedback in the response on what server id was used to generate each response)

    TODO: Must also verify that during leader elections, clients are smart enough to wait/retry for a reasonable amount of time instead of erroring out (this counts reads and writes)

    TODO: Test that even if InstallSnapshot returns an error (after succeeding), the ConsensusModule will still make forward progress by retrying via an AppendEntries request to pull the current state of the log.
    */

    let mut status_server = rpc::Http2Server::new(Some(8000));
    status_server.add_profilez()?;
    let status_server = status_server.start();

    let cluster = TestMetastoreCluster::create().await?;

    let mut node0 = cluster.start_node(0, true).await?;

    let client0 = node0.create_client().await?;
    client0.put(b"/a/1", b"hello").await?;
    client0.put(b"/a/2", b"world").await?;

    client0.close().await?;
    node0.close().await?;

    executor::sleep(Duration::from_millis(100)).await;

    let node0 = cluster.start_node(0, false).await?;

    // Wait to be re-elected
    executor::sleep(Duration::from_millis(400)).await?;

    let client0 = node0.create_client().await?;

    // A bit of a stress test to verify that streams are being cleaned up in the
    // HTTP2 code (the max number of concurrent streams is 200).

    for i in 0..200 {
        assert_eq!(client0.get(b"/a/blah").await?, None);
    }

    for i in 0..200 {
        assert_eq!(client0.get(b"/a/1").await?, Some(b"hello".to_vec()));
        assert_eq!(client0.get(b"/a/2").await?, Some(b"world".to_vec()));
    }

    // TODO: Grab a read index here and verify that much later we can still read
    // this data.

    // Verify only one server initially
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 1 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client0.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    let node1 = cluster.start_node(1, false).await?;

    // Wait long enough for node #1 to be discovered and join the group but not too
    // long that it has been promoted yet.
    executor::sleep(Duration::from_millis(200)).await;

    // Second server should start out as ASPIRING
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 1 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 6 }
                        role: ASPIRING
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client0.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    assert!(node0
        .dir_contents()
        .await?
        .contains(&"log/00000001".to_string()));
    assert!(node1
        .dir_contents()
        .await?
        .contains(&"log/00000001".to_string()));

    let mut all_keys = vec![];
    for i in 0..200 {
        let mut data = vec![0u8; 20 * 1024];

        let start = Instant::now();

        let key = format!("/a/{}", i).into_bytes();
        client0.put(&key, &data).await?;
        all_keys.push(key);

        let end = Instant::now();

        println!("Key #{} took {:?}", i, end - start);

        executor::sleep(Duration::from_millis(50)).await;
    }

    // Second server should be a MEMBER by this point
    // TODO: Verify that this will happen regardless of whether or not we just wrote
    // a bunch of keys.
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 1 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 6 }
                        role: MEMBER
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client0.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    // Wait for discards to complete. Currently this takes a while since the config
    // snapshot is only flushed every once in a while.
    executor::sleep(Duration::from_secs(10)).await;

    println!("{:?}", node0.dir_contents().await?);
    println!("{:?}", node1.dir_contents().await?);

    // Verify that the log has been truncated (re-starting a node or adding a new
    // member will require restoring a snapshot).
    assert!(!node0
        .dir_contents()
        .await?
        .contains(&"log/00000001".to_string()));
    assert!(!node1
        .dir_contents()
        .await?
        .contains(&"log/00000001".to_string()));

    client0.close().await?;

    let client = cluster.create_client().await?;

    all_keys.sort();

    {
        let mut data = client.get_prefix(b"/").await?;
        assert_eq!(data.len(), 200);
        data.sort_by_key(|e| e.key().to_vec());
        assert_eq!(
            &data.iter().map(|e| e.key().to_vec()).collect::<Vec<_>>(),
            &all_keys
        );
    }

    // Close and re-open the nodes

    node0.close().await?;
    node1.close().await?;

    let node0 = cluster.start_node(0, false).await?;
    let node1 = cluster.start_node(1, false).await?;

    // Wait for leader election.
    executor::sleep(Duration::from_secs(1)).await;

    // Waiting for new metadata to be propagated to the client instance.
    // (multicast broadcasts every 2 seconds)
    executor::sleep(Duration::from_secs(4)).await;

    // Verify data is still intact after the restart.
    //
    // NOTE: We are re-using the same client as from before the restart of all the
    // nodes to verify that client instances can eventually recover from full
    // cluster restart (which will re-assign new addresses/ports to all the nodes).
    {
        let mut data = client.get_prefix(b"/").await?;
        assert_eq!(data.len(), 200);
        data.sort_by_key(|e| e.key().to_vec());
        assert_eq!(
            &data.iter().map(|e| e.key().to_vec()).collect::<Vec<_>>(),
            &all_keys
        );
    }

    // TODO: Check that node1 is now the leader (which should almost always be the
    // case since it starts second)

    let node2 = cluster.start_node(2, false).await?;

    // Wait for third node to join
    executor::sleep(Duration::from_millis(600)).await;

    // Third server should start out as ASPIRING
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 6 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 6 }
                        role: MEMBER
                    },
                    {
                        id { value: 209 }
                        role: ASPIRING
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    // Writes still working while promoting the member.
    // TODO: Ideally do this sooner after start_node is called.
    for i in 0..20 {
        let key = format!("/a/{}", i).into_bytes();
        client.put(&key, &[0]).await?;
        executor::sleep(Duration::from_millis(100)).await;
    }

    // Should of gotten a snapshot and become a member by now.
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 6 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 6 }
                        role: MEMBER
                    },
                    {
                        id { value: 209 }
                        role: MEMBER
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    // Writes still working in the steady state with 3 nodes.
    for i in 0..20 {
        let key = format!("/a/{}", i).into_bytes();
        client.put(&key, &[0]).await?;
        executor::sleep(Duration::from_millis(100)).await;
    }

    // Things still generally working.
    {
        let mut data = client.get_prefix(b"/").await?;
        assert_eq!(data.len(), 200);
        data.sort_by_key(|e| e.key().to_vec());
        assert_eq!(
            &data.iter().map(|e| e.key().to_vec()).collect::<Vec<_>>(),
            &all_keys
        );
    }

    node0.close().await?;

    // Writes still working with only 2 of 3 nodes.
    // TODO: Verify that requests are indeed failing to hit node0 (e.g. check no
    // longer syncronized)
    for i in 0..20 {
        let key = format!("/a/{}", i).into_bytes();

        let start = Instant::now();

        // TODO: Make sure that calling this on a shutdown client immedialtey fails.
        client.put(&key, &[i]).await?;
        let end = Instant::now();

        println!("Key #{} took {:?}", i, end - start);

        executor::sleep(Duration::from_millis(100)).await;

        let value = client.get(&key).await?;
        assert_eq!(value, Some(vec![i]));

        executor::sleep(Duration::from_millis(100)).await;
    }

    let node0 = cluster.start_node(0, false).await?;

    // Check that current leader is server id #6 (the second one)
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            id { value: 6 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 6 }
                        role: MEMBER
                    },
                    {
                        id { value: 209 }
                        role: MEMBER
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client.current_status().await?;
        assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    // TODO: Verify that this leader transitoin transition is smoth
    client.remove_server(ServerId::from(6)).await?;

    // Verify things still working.
    for i in 0..20 {
        let key = format!("/a/{}", i).into_bytes();

        let start = Instant::now();

        // TODO: Make sure that calling this on a shutdown client immedialtey fails.
        client.put(&key, &[2 * i]).await?;
        let end = Instant::now();

        println!("Key #{} took {:?}", i, end - start);

        executor::sleep(Duration::from_millis(10)).await;

        let value = client.get(&key).await?;
        assert_eq!(value, Some(vec![2 * i]));

        executor::sleep(Duration::from_millis(10)).await;
    }

    // Verify that the server was removed from the configuration.
    // NOTE: It is undefined who will be the new leader.
    {
        let mut expected_status = Status::default();
        protobuf::text::parse_text_proto(
            r#"
            # id { value: 6 }
            role: LEADER
            configuration {
                servers: [
                    {
                        id { value: 1 }
                        role: MEMBER
                    },
                    {
                        id { value: 209 }
                        role: MEMBER
                    }
                ]
            }
            "#,
            &mut expected_status,
        )?;

        let status = client.current_status().await?;
        // assert_eq!(status.id(), expected_status.id());
        assert_eq!(status.role(), expected_status.role());
        assert_eq!(status.configuration(), expected_status.configuration());
    }

    // TODO: Start 8 concurrent writes and have them all spamming at the same time.

    // let client =

    client.close().await?;

    node0.close().await?;
    node1.close().await?;
    node2.close().await?;

    Ok(())
}
