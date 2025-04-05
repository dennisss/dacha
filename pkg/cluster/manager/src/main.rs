// Binary executed by the manager workers in the cluster which start jobs and
// watch over workers.

#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    cluster_manager::entry::main().await
}
