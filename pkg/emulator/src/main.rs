extern crate emulator;

macro_rules! sss {
    ($name:ident) => { stringify!($name,dfdf) };
}

fn main() -> emulator::errors::Result<()> {
//	println!("{}", sss!(hello));

	emulator::gameboy::run()
}