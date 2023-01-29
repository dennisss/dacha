extern crate common;
extern crate emulator;
#[macro_use]
extern crate macros;

use common::errors::*;

macro_rules! sss {
    ($name:ident) => {
        stringify!($name, dfdf)
    };
}

#[executor_main]
async fn main() -> Result<()> {
    //	println!("{}", sss!(hello));

    emulator::gameboy::run().await
}
