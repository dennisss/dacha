#[macro_use]
extern crate common;
extern crate study;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    study::run().await
}
