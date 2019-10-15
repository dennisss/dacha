#![feature(trait_alias)]
#[macro_use] extern crate error_chain;
extern crate fs2;
extern crate libc;

pub mod fs;
pub mod algorithms;
pub mod factory;
pub mod bits;

pub trait FlipSign<T> {
	/// Transmutes an signed/unsigned integer into it's opposite unsigned/signed integer while maintaining bitwise equivalence even though the integer value may change
	/// 
	/// We use this rather than directly relying on 'as' inline to specify times when we intentionally don't care about the value over/underflowing upon reinterpretation of the bits in a different sign
	fn flip(self) -> T;
}

impl FlipSign<u16> for i16 { fn flip(self) -> u16 { self as u16 } }
impl FlipSign<i16> for u16 { fn flip(self) -> i16 { self as i16 } }
impl FlipSign<u32> for i32 { fn flip(self) -> u32 { self as u32 } }
impl FlipSign<i32> for u32 { fn flip(self) -> i32 { self as i32 } }
impl FlipSign<u64> for i64 { fn flip(self) -> u64 { self as u64 } }
impl FlipSign<i64> for u64 { fn flip(self) -> i64 { self as i64 } }


pub mod errors {
	#[derive(Debug)]
	pub enum BitIoErrorKind {
		/// Occurs when reading from a BitReader and the input stream runs out of bits before the read was complete.
		NotEnoughBits
	}

	#[derive(Debug)]
	pub enum ParserErrorKind {
		Incomplete
	}

	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			ParseInt(::std::num::ParseIntError);
			// Db(diesel::result::Error);
			// HTTP(hyper::Error);
		}

		errors {
			// A type of error returned while performing a request
			// It is generally appropriate to respond with this text as a 400 error
			// We will eventually standardize the codes such that higher layers can easily distinguish errors
			// API(code: u16, message: &'static str) {
			// 	display("API Error: {} '{}'", code, message)
			// }

			BitIo(t: BitIoErrorKind) {
				display("BitIo: {:?}", t)
			}
			Parser(t: ParserErrorKind) {
				display("Parser: {:?}", t)
			}
		}
	}
}

pub trait FutureResult<T> = std::future::Future<Output=errors::Result<T>>;

pub fn ceil_div(a: usize, b: usize) -> usize {
	let mut out = a / b;
	if a % b != 0 {
		out += 1;
	}

	out
}

/// Given that the current position in the file is at the end of a middle, this will determine how much 
pub fn block_size_remainder(block_size: u64, end_offset: u64) -> u64 {
	let rem = end_offset % block_size;
	if rem == 0 {
		return 0;
	}

	(block_size - rem)
}

#[macro_export]
macro_rules! tup {
	(($a:ident, $b:ident) = $e:expr) => {{
		let tmp = $e;
		$a = tmp.0;
		$b = tmp.1;
	}};
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn block_size_remainder_test() {
		let bsize = 64;
		assert_eq!(block_size_remainder(bsize, 0), 0);
		assert_eq!(block_size_remainder(bsize, 3*bsize), 0);
		assert_eq!(block_size_remainder(bsize, bsize - 4), 4);
		assert_eq!(block_size_remainder(bsize, 6*bsize + 5), bsize - 5);
	}

}

