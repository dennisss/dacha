#![feature(trait_alias, core_intrinsics)]

#[macro_use] extern crate arrayref;
extern crate bytes;

use bytes::Bytes;
use common::errors::*;

pub mod iso;
pub mod binary;

pub fn is_one_of(s: &str, input: u8) -> bool {
	s.bytes().find(|x| *x == input).is_some()
}

pub fn one_of(s: &'static str) -> impl Parser<u8> {
	like(move |i| is_one_of(s, i))
}


pub type ParseError = Error;
pub type ParseResult<T> = std::result::Result<(T, Bytes), ParseError>;
pub trait Parser<T> = Fn(Bytes) -> ParseResult<T>;

#[macro_export]
macro_rules! alt {
	( $first:expr, $( $next:expr ),* ) => {
		|input: Bytes| {
			let mut errs = vec![];
			match ($first)(input.clone()) {
				Ok(v) => { return Ok(v); },
				Err(e) => { errs.push(e.to_string()); }
			};

			$(
			match ($next)(input.clone()) {
				Ok(v) => { return Ok(v); },
				Err(e) => { errs.push(e.to_string()); }
			};
			)*

			Err(format!("({})", errs.join(" | ")).into())
		}
	};
}

// TODO: Try to convert to a function taking a lambda with an &mut ParserCursor parameter
#[macro_export]
macro_rules! seq {
	($c: ident => $e:expr) => {
		move |input| {
			let mut $c = ParseCursor::new(input);
			let out: std::result::Result<_, ParseError> = { $e };
			$c.unwrap_with(out?)
		}
	};
}

// See https://stackoverflow.com/questions/38088067/equivalent-of-func-or-function-in-rust
#[macro_export]
macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            extern crate core;
            unsafe { core::intrinsics::type_name::<T>() }
        }
        let name = type_name_of(f);
        &name[6..name.len() - 16]
    }}
}

#[macro_export]
macro_rules! parser {
	(pub $c:ident<$t:ty> => $e:expr) => {
		pub fn $c(input: Bytes) -> ParseResult<$t> {
			let p = $e;
			p(input).map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
	// Same thing as the first form, but not public.
	($c:ident<$t:ty> => $e:expr) => {
		fn $c(input: Bytes) -> ParseResult<$t> {
			let p = $e;
			p(input).map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
}

pub fn map<T, Y, P: Parser<T>, F: Fn(T) -> Y>(p: P, f: F) -> impl Parser<Y> {
	move |input: Bytes| {
		p(input).map(|(v, rest)| (f(v), rest))
	}
}

pub fn and_then<T, Y, P: Parser<T>, F: Fn(T) -> std::result::Result<Y, ParseError>>(p: P, f: F) -> impl Parser<Y> {
	move |input: Bytes| {
		p(input).and_then(|(v, rest)| Ok((f(v)?, rest)))
	}
}


pub fn like<F: Fn(u8) -> bool>(f: F) -> impl Parser<u8> {
	move |input: Bytes| {
		if input.len() < 1 {
			return Err("like: too little input".into());
		}

		if f(input[0]) {
			Ok((input[0], input.slice(1..)))
		} else {
			Err("like failed".into())
		}
	}
}

parser!(pub any<u8> => like(|_| true)); 

pub fn complete<T, P: Parser<T>>(p: P) -> impl Parser<T> {
	move |input: Bytes| {
		p(input).and_then(|(v, rest)| {
			if rest.len() != 0 {
				Err(format!("Failed to parse last {} bytes", rest.len()).into())
			} else {
				Ok((v, rest))
			}
		})
	}
}

/// Takes a specified number of bytes from the input
pub fn take_exact(length: usize) -> impl Parser<Bytes> {
	move |input: Bytes| {
		if input.len() < length {
			return Err("Not enough bytes in input".into());
		}

		let mut v = input.clone();
		let rest = v.split_off(length);
		Ok((v, rest))
	}
}

pub fn take_while<F: Fn(u8) -> bool>(f: F) -> impl Parser<Bytes> {
	move |input: Bytes| {
		let mut i = 0;
		while i < input.len() {
			if !f(input[i]) {
				break;
			}
			i += 1;
		}

		let mut v = input.clone();
		let rest = v.split_off(i);
		Ok((v, rest))
	}
}

pub fn take_while1<F: Fn(u8) -> bool>(f: F) -> impl Parser<Bytes> {
	let p = take_while(f);
	move |input: Bytes| {
		let (v, rest) = p(input)?;
		if v.len() > 0 {
			Ok((v, rest))
		} else {
			Err("take_while1 failed".into())
		}
	}
}

pub fn tag(s: &'static str) -> impl Parser<Bytes> {
	move |input: Bytes| {
		if input.len() < s.len() {
			return Err(format!("tag expected: {}", s).into());
		}

		if &input[0..s.len()] == s.as_bytes() {
			let mut v = input.clone();
			let rest = v.split_off(s.len());
			Ok((v, rest))
		} else {
			Err(format!("Expected \"{}\"", s).into())
		}
	}
}

pub fn opt<T, P: Parser<T>>(p: P) -> impl Parser<Option<T>> {
	move |input: Bytes| {
		match p(input.clone()) {
			Ok((v, rest)) => Ok((Some(v), rest)),
			Err(_) => Ok((None, input))
		}
	}
}

pub fn many<T, F: Parser<T>>(f: F) -> impl Parser<Vec<T>> {
	move |input: Bytes| {
		let mut results = vec![];
		let mut rest = input.clone();
		while let Ok((v, r)) = f(rest.clone()) {
			rest = r;
			results.push(v);
		}

		Ok((results, rest))
	}
}

pub fn many1<T, F: Parser<T>>(f: F) -> impl Parser<Vec<T>> {
	let p = many(f);
	move |input: Bytes| {
		let (vec, rest) = p(input)?;
		if vec.len() < 1 {
			return Err("many1 failed".into());
		}

		Ok((vec, rest))
	}
}




/// Given a parser, output a parser which outputs the entire range of the input that is consumed by the parser if successful.
pub fn slice<T, P: Parser<T>>(p: P) -> impl Parser<Bytes> {
	move |input: Bytes| {
		let (_, rest) = p(input.clone())?;
		let n = input.len() - rest.len();
		Ok((input.slice(0..n), rest))
	}
}


#[derive(Clone)]
pub struct ParseCursor {
	rest: Bytes
}

impl ParseCursor {
	pub fn new(input: Bytes) -> ParseCursor {
		ParseCursor { rest: input.clone() }
	}
	
	pub fn unwrap_with<T>(self, value: T) -> ParseResult<T> {
		Ok((value, self.rest))
	}

	// Runs a parser on the remaining input, advancing the cursor if successful.
	// On parser error, the cursor will stay the same.
	pub fn next<T, F: Parser<T>>(&mut self, f: F) -> std::result::Result<T, ParseError> {
		match f(self.rest.clone()) {
			Ok((v, r)) => {
				self.rest = r;
				Ok(v)
			},
			Err(e) => Err(e)
		}
	}

	pub fn is<T: PartialEq<Y>, Y, F: Parser<T>>(&mut self, f: F, v: Y) -> std::result::Result<T, ParseError> {
		match f(self.rest.clone()) {
			Ok((v2, r)) => {
				if v2 != v {
					return Err("is failed".into());
				}

				self.rest = r;
				Ok(v2)
			},
			Err(e) => Err(e)
		}
	}

	// Runs a parser as many times as possible returning a vector of all results
	pub fn many<T, F: Parser<T>>(&mut self, f: F) -> Vec<T> {
		let mut results = vec![];
		while let Ok((v, r)) = f(self.rest.clone()) {
			self.rest = r;
			results.push(v);
		}

		results
	}
}
