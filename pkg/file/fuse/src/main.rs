#[macro_use]
extern crate macros;

use base_error::*;
use executor_multitask::ServiceResource;

#[executor_main]
async fn main() -> Result<()> {
    let server = fuse::Server::create(file::LocalPath::new("/tmp/fuse_test")).await?;

    server.wait_for_termination().await?;

    Ok(())
}
