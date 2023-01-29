// Binary executed by the manager workers in the cluster which start jobs and
// watch over workers.

extern crate common;
extern crate container;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    container::manager_main().await
}
