#![feature(
    trait_alias,
    const_fn,
    const_constructor,
    associated_type_defaults,
    str_strip,
    specialization
)]
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate failure;
pub extern crate async_std;
extern crate base64;
pub extern crate bytes;
extern crate fs2;
pub extern crate futures;
pub extern crate hex;
extern crate libc;
#[macro_use]
extern crate lazy_static;
extern crate generic_array;
extern crate typenum;

pub mod algorithms;
pub mod args;
pub mod async_fn;
pub mod bits;
pub mod bundle;
pub mod collections;
pub mod factory;
pub mod fs;
pub mod io;
pub mod iter;
pub mod line_builder;
pub mod vec;

pub use async_trait::*;

pub async fn wait_for(dur: std::time::Duration) {
    let never = async_std::future::pending::<()>();
    async_std::future::timeout(dur, never).await.unwrap_or(());
}

//pub use async_std;
//pub use futures;

pub trait FlipSign<T> {
    /// Transmutes an signed/unsigned integer into it's opposite unsigned/signed
    /// integer while maintaining bitwise equivalence even though the integer
    /// value may change
    ///
    /// We use this rather than directly relying on 'as' inline to specify times
    /// when we intentionally don't care about the value over/underflowing upon
    /// reinterpretation of the bits in a different sign
    fn flip(self) -> T;
}

impl FlipSign<u16> for i16 {
    fn flip(self) -> u16 {
        self as u16
    }
}
impl FlipSign<i16> for u16 {
    fn flip(self) -> i16 {
        self as i16
    }
}
impl FlipSign<u32> for i32 {
    fn flip(self) -> u32 {
        self as u32
    }
}
impl FlipSign<i32> for u32 {
    fn flip(self) -> i32 {
        self as i32
    }
}
impl FlipSign<u64> for i64 {
    fn flip(self) -> u64 {
        self as u64
    }
}
impl FlipSign<i64> for u64 {
    fn flip(self) -> i64 {
        self as i64
    }
}

pub trait LeftPad<T> {
    fn left_pad(self, size: usize, default_value: T) -> Vec<T>;
}

impl<T: Copy> LeftPad<T> for Vec<T> {
    fn left_pad(mut self, size: usize, default_value: T) -> Self {
        if self.len() == size {
            return self;
        }

        let mut out = vec![];
        out.reserve(std::cmp::max(size, self.len()));

        for _ in self.len()..size {
            out.push(default_value);
        }

        out.append(&mut self);
        out
    }
}

pub mod errors {
    pub use failure::err_msg;
    pub use failure::format_err;
    pub use failure::Error;

    pub type Result<T> = std::result::Result<T, Error>;
}

/*
pub mod errors {
    error_chain! {
        foreign_links {
            Io(::std::io::Error);
            ParseInt(::std::num::ParseIntError);
            CharTryFromError(::std::char::CharTryFromError);
            FromUtf8Error(::std::str::Utf8Error);
            FromUtf8ErrorString(::std::string::FromUtf8Error);
            FromUtf16Error(::std::string::FromUtf16Error);
            FromHexError(::hex::FromHexError);
            FromBase64Error(::base64::DecodeError);
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
            Parser(t: ParserErrorKind) {
                display("Parser: {:?}", t)
            }
        }
    }
}
*/

pub trait FutureResult<T> = std::future::Future<Output = errors::Result<T>>;

pub fn ceil_div(a: usize, b: usize) -> usize {
    let mut out = a / b;
    if a % b != 0 {
        out += 1;
    }

    out
}

/// Given that the current position in the file is at the end of a middle, this
/// will determine how much
pub fn block_size_remainder(block_size: u64, end_offset: u64) -> u64 {
    let rem = end_offset % block_size;
    if rem == 0 {
        return 0;
    }

    (block_size - rem)
}

pub fn camel_to_snake_case(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_alphabetic() && c.is_ascii_uppercase() {
            s.push('_');
        }

        s.push(c.to_ascii_lowercase());
    }

    s
}

pub trait InRange {
    fn in_range(self, min: Self, max: Self) -> bool;
}

impl<T: PartialOrd> InRange for T {
    fn in_range(self, min: Self, max: Self) -> bool {
        self >= min && self <= max
    }
}

#[macro_export]
macro_rules! tup {
    (($a:ident, $b:ident) = $e:expr) => {{
        let tmp = $e;
        $a = tmp.0;
        $b = tmp.1;
    }};
}

#[macro_export]
macro_rules! enum_def {
    ($name:ident $t:ty => $( $case:ident = $val:expr ),*) => {
    	#[derive(Clone, Copy, PartialEq, Eq, Debug)]
		pub enum $name {
			$(
				$case = $val
			),*
		}

		impl $name {
			pub fn from_value(v: $t) -> Result<Self> {
				Ok(match v {
					$(
						$val => $name::$case,
					)*
					_ => {
						return Err(
							format_err!("Unknown value for '$name': {}", v));
					}
				})
			}

			pub fn to_value(&self) -> $t {
				match self {
					$(
						$name::$case => $val,
					)*
				}
			}
		}

    };
}

/// Implements Deref and DerefMut for the simple case of which the derefernced
/// value is a direct field of the struct.
///
/// Usage:
/// struct Wrapper { apple: Apple }
/// impl_deref!(Wrapper::apple as Apple);
///
/// wrapper.deref()   <- This is of type &Apple
#[macro_export]
macro_rules! impl_deref {
    ($name:ident :: $field:ident as $t:ty) => {
        impl ::std::ops::Deref for $name {
            type Target = $t;

            fn deref(&self) -> &Self::Target {
                &self.$field
            }
        }

        impl ::std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.$field
            }
        }
    };
}

#[macro_export]
macro_rules! min_size {
    ($s:ident, $size:expr) => {
        if $s.len() < $size {
            return Err(err_msg("Input buffer too small"));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_size_remainder_test() {
        let bsize = 64;
        assert_eq!(block_size_remainder(bsize, 0), 0);
        assert_eq!(block_size_remainder(bsize, 3 * bsize), 0);
        assert_eq!(block_size_remainder(bsize, bsize - 4), 4);
        assert_eq!(block_size_remainder(bsize, 6 * bsize + 5), bsize - 5);
    }
}
