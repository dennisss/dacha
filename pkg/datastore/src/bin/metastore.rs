/*

Example:
    cargo run --bin metastore -- --dir=data/meta1 --init_port=4000 --port=4001
    cargo run --bin metastore -- --dir=data/meta2 --init_port=4000 --port=4002

Bootstrap using:
    cargo run --package rpc_util -- call 127.0.0.1:30001 ServerInit.Bootstrap ''

cargo run --package rpc_util -- ls 127.0.0.1:4001

cargo run --package rpc_util -- call 127.0.0.1:4001 KeyValueStore.Get 'data: "hello"'
*/

#[macro_use]
extern crate macros;

use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::async_std::path::PathBuf;
use common::async_std::task::block_on;
use common::errors::*;
use rpc_util::NamedPortArg;

use datastore::meta::store::{run, MetastoreConfig};
use raft::proto::routing::RouteLabel;

#[derive(Args)]
struct Args {
    init_port: NamedPortArg,
    port: NamedPortArg,
    dir: PathBuf,
    labels: CommaSeparated<String>,
}

fn main() -> Result<()> {
    let args = parse_args::<Args>()?;

    let mut route_labels = vec![];
    for label in args.labels.values {
        let mut l = RouteLabel::default();
        l.set_value(label);
        route_labels.push(l);
    }

    block_on(run(&MetastoreConfig {
        dir: args.dir,
        init_port: args.init_port.value(),
        service_port: args.port.value(),
        route_labels,
    }))
}
