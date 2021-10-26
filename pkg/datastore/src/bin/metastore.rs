/*

Example:
    cargo run --bin metastore -- --dir=data/meta1 --init_port=4000 --port=4001
    cargo run --bin metastore -- --dir=data/meta2 --init_port=4000 --port=4002

Bootstrap using:
    cargo run --package rpc_util -- call 127.0.0.1:4000 ServerInit.Bootstrap ''

cargo run --package rpc_util -- ls 127.0.0.1:4001

cargo run --package rpc_util -- call 127.0.0.1:4001 KeyValueStore.Get 'data: "hello"'
*/

#[macro_use]
extern crate macros;

use common::args::parse_args;
use common::async_std::path::PathBuf;
use common::async_std::task::block_on;
use common::errors::*;

use datastore::meta::store::{run, MetaStoreConfig};

#[derive(Args)]
struct Args {
    init_port: u16,
    port: u16,
    dir: PathBuf,
}

fn main() -> Result<()> {
    let args = parse_args::<Args>()?;

    block_on(run(&MetaStoreConfig {
        dir: args.dir,
        init_port: args.init_port,
        service_port: args.port,
    }))
}
