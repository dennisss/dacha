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
