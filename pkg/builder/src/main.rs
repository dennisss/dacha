extern crate builder;
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    builder::cli::run().await
}
