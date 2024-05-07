/*

Example:
    cargo run --release --bin metastore -- --dir=/tmp/meta1 --init_port=4000 --port=4001
    cargo run --release --bin metastore -- --dir=/tmp/meta2 --init_port=5000 --port=5002

Bootstrap using:
    cargo run --package rpc_util -- call 127.0.0.1:4000 ServerInit.Bootstrap '' --insecure

cargo run --package rpc_util -- ls 127.0.0.1:4001 --insecure

cargo run --package rpc_util -- call 127.0.0.1:4001 KeyValueStore.Get 'data: "hello"' --insecure
*/

#[macro_use]
extern crate macros;

use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::errors::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use rpc_util::NamedPortArg;

use datastore::meta::{
    store::{run, MetastoreOptions},
    EmbeddedDBStateMachineOptions,
};
use raft::{log::segmented_log::SegmentedLogOptions, proto::RouteLabel};

#[derive(Args)]
struct Args {
    /// Port on which the consensus initialization service will be hosted (if
    /// the server isn't already initialized).
    init_port: Option<NamedPortArg>,

    port: NamedPortArg,

    dir: LocalPathBuf,
    labels: CommaSeparated<String>,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = parse_args::<Args>()?;

    let mut route_labels = vec![];
    for label in args.labels.values {
        let mut l = RouteLabel::default();
        l.set_value(label);
        route_labels.push(l);
    }

    let root = RootResource::new();

    root.register_dependency(
        run(MetastoreOptions {
            dir: args.dir,
            init_port: args.init_port.as_ref().map(|v| v.value()),
            bootstrap: false,
            service_port: args.port.value(),
            route_labels,
            log: SegmentedLogOptions::default(),
            state_machine: EmbeddedDBStateMachineOptions::default(),
        })
        .await?,
    )
    .await;

    root.wait().await
}
