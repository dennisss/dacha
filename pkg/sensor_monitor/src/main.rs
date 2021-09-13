#[macro_use]
extern crate common;
extern crate sensor_monitor;

use common::errors::*;

fn main() -> Result<()> {
    common::async_std::task::block_on(sensor_monitor::run())
}
