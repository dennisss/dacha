#[macro_use]
extern crate common;
extern crate sensor_monitor;

use common::errors::*;

fn main() -> Result<()> {
    executor::run(sensor_monitor::run())?
}
