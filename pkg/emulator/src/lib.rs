#[macro_use] extern crate common;
#[macro_use] extern crate failure;
extern crate minifb;

#[macro_use] extern crate arrayref;

pub mod errors {
	pub use failure::Error;
	pub use failure::err_msg;

	pub type Result<T> = std::result::Result<T, Error>;
}

pub mod gameboy;