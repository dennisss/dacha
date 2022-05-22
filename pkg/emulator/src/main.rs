extern crate common;
extern crate emulator;

use common::errors::*;

macro_rules! sss {
    ($name:ident) => {
        stringify!($name, dfdf)
    };
}

fn main() -> Result<()> {
    //	println!("{}", sss!(hello));

    common::async_std::task::block_on(emulator::gameboy::run())
}
