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

    executor::run(emulator::gameboy::run())?
}
