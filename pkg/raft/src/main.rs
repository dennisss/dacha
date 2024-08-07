#![feature(
    proc_macro_hygiene,
    decl_macro,
    type_alias_enum_variants,
    async_closure
)]

extern crate raft;
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

mod key_value;
mod mongodb;
mod redis;

use std::sync::Arc;

use common::errors::*;
use common::errors::*;
use common::futures::future::*;
use common::futures::prelude::*;
use executor_multitask::RootResource;
use file::dir_lock::DirLock;
use file::LocalPathBuf;
use protobuf::Message;
use raft::log::segmented_log::SegmentedLogOptions;
use raft::node::*;
use raft::proto::*;
use raft::server::server::{Server, ServerInitialState};

use key_value::*;
use redis::resp::*;

/*
    Benchmark using:
    -	 redis-benchmark -t set,get -n 100000 -q -p 12345

    - In order to beat the 'set' benchmark, we must demonstrate efficient pipelining of all the concurrent requests to append an entry
        -

    Command for testing:
    - redis-cli -p 5001
*/

/*
    Some form of client interface is needed so that we can forward arbitrary entries to any server

*/

// XXX: See https://github.com/etcd-io/etcd/blob/fa92397e182286125c72bf52d95f9f496f733bdf/raft/raft.go#L113 for more useful config parameters

/*
    Other scenarios
    - Server startup
        - Server always starts completely idle and in a mode that would reject external requests
        - If we have configuration on disk already, then we can use that
        - If we start with a join cli flag, then we can:
            - Ask the cluster to create a new unique machine id (we could trivially use an empty log entry and commit that to create a new id) <- Must make sure this does not conflict with the master's id if we make many servers before writing other data

        - If we are sent a one-time init packet via http post, then we will start a new cluster on ourselves
*/

/*
    Summary of event variables:
    - OnCommited
        - Ideally this would be a channel tht can pass the Arc references to the listeners so that maybe we don't need to relock in order to take things out of the log
        - ^ This will be consumed by clients waiting on proposals to be written and by the state machine thread waiting for the state machine to get fully applied
    - OnApplied
        - Waiting for when a change is applied to the state machine
    - OnWritten
        - Waiting for when a set of log entries have been persisted to the log file
    - OnStateChange
        - Mainly to wake up the cycling thread so that it can
        - ^ This will always only have a single consumer so this may always be held as light weight as possibl

    TODO: Future optimization would be to also save the metadata into the log file so that we are only ever writing to one append-only file all the time
        - I think this is how etcd implements it as well
*/

struct RaftRedisServer {
    node: Arc<raft::Node<KeyValueReturn>>,
    state_machine: Arc<MemoryKVStateMachine>,
}

use redis::resp::RESPObject;
use redis::resp::RESPString;

#[async_trait]
impl redis::server::Service for RaftRedisServer {
    async fn get(&self, key: RESPString) -> Result<RESPObject> {
        let state_machine = &self.state_machine;

        self.node
            .server()
            .begin_read(true)
            .await
            .map_err(|_| err_msg("Not leader"))?;

        let val = state_machine.get(key.as_ref()).await;

        Ok(match val {
            // NOTE: THis implies that we have no efficient way to serialize
            // from references anyway
            Some(v) => RESPObject::BulkString(v),
            None => RESPObject::Nil,
        })
    }

    // TODO: What is the best thing to do on errors?
    async fn set(&self, key: RESPString, value: RESPString) -> Result<RESPObject> {
        let mut op = KeyValueOperation::default();
        op.set_mut().set_key(key.as_ref().to_vec());
        op.set_mut().set_value(value.as_ref().to_vec());

        let mut entry = LogEntryData::default();
        entry.set_command(op.serialize()?);

        self.node
            .server()
            .execute(entry)
            .await
            .map_err(|e| format_err!("SET failed with error: {:?}", e))?;

        Ok(RESPObject::SimpleString(b"OK"[..].into()))
    }

    async fn del(&self, key: RESPString) -> Result<RESPObject> {
        // TODO: This requires knowledge of how many keys were actually deleted
        // (for the case of non-existent keys)

        let mut op = KeyValueOperation::default();
        op.delete_mut().set_key(key.as_ref().to_vec());

        let mut entry = LogEntryData::default();
        entry.set_command(op.serialize()?);

        let pending_exec = self
            .node
            .server()
            .execute(entry)
            .await
            .map_err(|e| format_err!("DEL failed with error: {:?}", e))?;

        let res = match pending_exec.wait().await {
            raft::PendingExecutionResult::Committed { value, .. } => {
                value.ok_or_else(|| err_msg("No result"))?
            }
            raft::PendingExecutionResult::Cancelled => {
                return Err(format_err!("Failed to commit DEL entry"));
            }
        };

        Ok(RESPObject::Integer(if res.success { 1 } else { 0 }))
    }

    async fn publish(&self, channel: &RESPString, object: &RESPObject) -> Result<usize> {
        Ok(0)
    }

    async fn subscribe(&self, channel: RESPString) -> Result<()> {
        Ok(())
    }

    async fn unsubscribe(&self, channel: RESPString) -> Result<()> {
        Ok(())
    }
}

/*

    XXX: DiscoveryService will end up requesting ourselves in the case of starting up the services themselves starting up
    -> Should be ideally topology agnostic
    -> We only NEED to do a discovery if we are not

    -> We always want to have a discovery service
        ->


    -> Every single server if given a seed list should try to reach that seed list on startup just to try and get itself in the cluster
        -> Naturally in the case of a bootstrap

    -> In most cases, if

*/

// NOTE: I still need to implement default values.
#[derive(Args)]
#[arg(desc = "Sample consensus reaching node")]
struct Args {
    #[arg(desc = "An existing directory to store data file for this unique instance")]
    dir: LocalPathBuf,

    // TODO: Also support specifying our rpc listening port
    #[arg(
        desc = "Address of a running server to be used for joining its cluster if this instance has not been initialized yet"
    )]
    join: Option<String>,

    #[arg(
        desc = "Indicates that this should be created as the first node in the cluster",
        default = false
    )]
    bootstrap: bool,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    // TODO: For now, we will assume that bootstrapping is well known up front
    // although eventually to enforce that it only ever occurs exactly once, we may
    // want to have an admin externally fire exactly one request to trigger it
    // But even if we do pass in bootstrap as an argument, it is still guranteed to
    // bootstrap only once on this machine as we will persistent the bootstrapped
    // configuration before talking to other servers in the cluster

    // TODO: Derive this based on an argument.
    // TODO: remove the http:// ?
    let seed_list: Vec<String> = vec![
        // "http://127.0.0.1:4001".into(),
        // "http://127.0.0.1:4002".into(),
    ];

    file::create_dir_all(&args.dir).await?;

    // XXX: Need to store this somewhere more persistent so that we don't lose it
    let lock = DirLock::open(&args.dir).await?;

    // XXX: Right here if we are able to retrieve a snapshot, then we are allowed to
    // do that But we will end up thinking of all the stuff initially on disk as
    // one atomic unit that is initially loaded
    let state_machine = Arc::new(MemoryKVStateMachine::new());

    let mut service = RootResource::new();

    let raft_port = 4000; // + node.id().value();
    let client_port = 5000; // + node.id().value();

    let mut rpc_server = rpc::Http2Server::new(Some(raft_port as u16));
    let rpc_server_address = format!("127.0.0.1:{}", raft_port);

    let mut node = Arc::new(
        Node::create(NodeOptions {
            dir: lock,
            init_port: Some(4000),
            bootstrap: args.bootstrap,
            seed_list,
            state_machine: state_machine.clone(),
            log_options: SegmentedLogOptions::default(),
            route_labels: vec![],
            rpc_server: &mut rpc_server,
            rpc_server_address,
        })
        .await?,
    );

    service.register_dependency(node.clone()).await;

    let client_server = Arc::new(redis::server::Server::new(RaftRedisServer {
        node,
        state_machine: state_machine.clone(),
    }));

    service.register_dependency(rpc_server.start()).await;

    service
        .spawn_interruptable(
            "redis::Server",
            redis::server::Server::run(client_server.clone(), client_port as u16),
        )
        .await;

    service.wait().await
}
