#[macro_use]
extern crate common;
extern crate sensor_monitor;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    sensor_monitor::run().await
}
