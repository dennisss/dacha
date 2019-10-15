#![feature(trait_alias, core_intrinsics)]

#[macro_use] extern crate arrayref;
extern crate bytes;

use crate::bytes::Buf;
use bytes::Bytes;
use common::errors::*;

pub fn incomplete_error() -> Error {
	ErrorKind::Parser(ParserErrorKind::Incomplete).into()
}

pub fn is_incomplete(e: &Error) -> bool {
	if let ErrorKind::Parser(ParserErrorKind::Incomplete) = e.kind() {
		true
	} else {
		false
	}
}

pub mod iso;
pub mod ascii;
pub mod binary;

pub fn is_one_of(s: &str, input: u8) -> bool {
	s.bytes().find(|x| *x == input).is_some()
}

pub fn one_of(s: &'static str) -> impl Parser<u8> {
	like(move |i| is_one_of(s, i))
}

pub type ParseError = Error;
pub type ParserInput = ::bytes::Bytes;
pub type ParseResult<T, I = ParserInput> = ParseResult2<T, I>;
pub type ParseResult2<T, R> = std::result::Result<(T, R), ParseError>;
pub trait Parser<T, I = ParserInput> = Fn(I) -> ParseResult<T, I>;

#[macro_export]
macro_rules! alt {
	( $first:expr, $( $next:expr ),* ) => {
		|input| {
			let mut errs = vec![];
			match ($first)(::std::clone::Clone::clone(&input)) {
				Ok(v) => { return Ok(v); },
				Err(e) => { errs.push(e.to_string()); }
			};

			$(
			match ($next)(::std::clone::Clone::clone(&input)) {
				Ok(v) => { return Ok(v); },
				Err(e) => { errs.push(e.to_string()); }
			};
			)*

			// TODO: Must support Incomplete error in some reasonable way.
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
			// Wrapped in a function so that the expression can have return statements.
			let mut f = || { $e };
			let out: std::result::Result<_, ParseError> = { f() };
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
            core::intrinsics::type_name::<T>()
        }
        let name = type_name_of(f);
        &name[6..name.len() - 3]
    }}
}

#[macro_export]
macro_rules! parser {
	(pub $c:ident<$t:ty> => $e:expr) => {
		parser!(pub $c<$t, ParserInput> => $e);
	};
	($c:ident<$t:ty> => $e:expr) => {
		parser!($c<$t, ParserInput> => $e);
	};

	(pub $c:ident<$t:ty, $r:ty> => $e:expr) => {
		pub fn $c(input: $r) -> ParseResult2<$t, $r> {
			let p = $e;
			p(input)
			
			// TODO: Must support passing through Incomplete error
			//.map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
	// Same thing as the first form, but not public.
	($c:ident<$t:ty, $r:ty> => $e:expr) => {
		fn $c(input: $r) -> ParseResult2<$t, $r> {
			let p = $e;
			p(input)
			// TODO: Must support passing through Incomplete error
			// .map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
}

pub fn map<I, T, Y, P: Parser<T, I>, F: Fn(T) -> Y>(p: P, f: F) -> impl Parser<Y, I> {
	move |input: I| {
		p(input).map(|(v, rest)| (f(v), rest))
	}
}

pub fn and_then<I, T, Y, P: Parser<T, I>, F: Fn(T) -> std::result::Result<Y, ParseError>>(p: P, f: F) -> impl Parser<Y, I> {
	move |input: I| {
		p(input).and_then(|(v, rest)| Ok((f(v)?, rest)))
	}
}

pub fn then<I, T, Y, P: Parser<T, I>, G: Parser<Y, I>>(p: P, g: G)
-> impl Parser<Y, I> {
	move |input: I| {
		p(input).and_then(|(_, rest)| g(rest))
	}
}

pub fn like<F: Fn(u8) -> bool>(f: F) -> impl Parser<u8> {
	move |input: ParserInput| {
		if input.len() < 1 {
			return Err(incomplete_error());
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
	move |input: ParserInput| {
		p(input).and_then(|(v, rest)| {
			if rest.len() != 0 {
				Err(format!("Failed to parse last {} bytes", rest.len()).into())
			} else {
				Ok((v, rest))
			}
		}).map_err(|e| {
			if is_incomplete(&e) {
				"Expected to be complete".into()
			} else {
				e
			}
		})
	}
}

/// Takes a specified number of bytes from the input
pub fn take_exact(length: usize) -> impl Parser<Bytes> {
	move |input: Bytes| {
		if input.len() < length {
			return Err(incomplete_error());
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

/// Continue parsing until the given parser is able to parse
/// NOTE: The parser must parse a non-empty pattern for this to work.
pub fn take_until<T, P: Parser<T>>(p: P) -> impl Parser<Bytes> {
	move |mut input: Bytes| {
		let mut rem = input.clone();
		while rem.len() > 0 {
			match p(rem.clone()) {
				Ok(_) => {
					input.advance(input.len() - rem.len());
					
					return Ok(( input.slice(0..(input.len() - rem.len())), rem));
				},
				Err(_) => rem.advance(1)
			}
		}

		// TODO: Return Incomplete error.
		Err("Hit end of input before seeing pattern".into())
	}
}

// TODO: Implement a binary version for efficient binary ASCII string parsing.
// (and refactor most ussages to use that)
pub fn tag<T: AsRef<[u8]>>(s: T) -> impl Parser<Bytes> {
	move |input: Bytes| {
		let t = s.as_ref();
		if input.len() < t.len() {
			return Err(incomplete_error());
			// return Err(format!("tag expected: {}", s).into());
		}

		if &input[0..t.len()] == t {
			let mut v = input.clone();
			let rest = v.split_off(t.len());
			Ok((v, rest))
		} else {
			Err(format!("Expected \"{:?}\"", t).into())
		}
	}
}

pub fn anytag<T: AsRef<[u8]>>(arr: &'static [T]) -> impl Parser<Bytes> {
	move |input: Bytes| {
		for t in arr.iter() {
			if let Ok(v) = tag(t)(input.clone()) {
				return Ok(v);
			}
		}

		// TODO: Incomplete errors?
		Err("No matching tag".into())
	}
}

pub fn opt<I: Clone, T, P: Parser<T, I>>(p: P) -> impl Parser<Option<T>, I> {
	move |input: I| {
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

/// Parses zero or more items delimited by another parser.
pub fn delimited<T, Y, P: Parser<T>, D: Parser<Y>>(p: P, d: D)
-> impl Parser<Vec<T>> {
	move |mut input| {
		let mut out = vec![];

		// Match first value.
		match p(std::clone::Clone::clone(&input)) {
			Ok((v, rest)) => {
				out.push(v);
				input = rest;
			},
			Err(e) => { return Ok((out, input)); }
		};

		loop {
			// Parse delimiter (but don't update 'input' yet).
			let rest = match d(std::clone::Clone::clone(&input)) {
				Ok((_, rest)) => rest,
				Err(_) => { return Ok((out, input)); }
			};

			match p(rest) {
				Ok((v, rest)) => {
					out.push(v);
					input = rest;
				},
				// On failure, return before the delimiter was parsed
				Err(_) => { return Ok((out, input)); }
			}
		}
	}
}

pub fn delimited1<T, Y, P: Parser<T>, D: Parser<Y>>(p: P, d: D)
-> impl Parser<Vec<T>> {
	and_then(delimited(p, d), |arr| {
		if arr.len() < 1 {
			Err("No items parseable".into())
		} else {
			Ok(arr)
		}
	})
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
pub struct ParseCursor<I: Clone = ParserInput> {
	rest: I
}

impl<I: Clone> ParseCursor<I> {
	pub fn new(input: I) -> Self {
		Self { rest: input.clone() }
	}
	
	pub fn unwrap_with<T>(self, value: T) -> ParseResult<T, I> {
		Ok((value, self.rest))
	}

	// Runs a parser on the remaining input, advancing the cursor if successful.
	// On parser error, the cursor will stay the same.
	pub fn next<T, F: Parser<T, I>>(&mut self, f: F) -> std::result::Result<T, ParseError> {
		match f(self.rest.clone()) {
			Ok((v, r)) => {
				self.rest = r;
				Ok(v)
			},
			Err(e) => Err(e)
		}
	}

	pub fn is<T: PartialEq<Y>, Y, F: Parser<T, I>>(&mut self, f: F, v: Y) -> std::result::Result<T, ParseError> {
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
	pub fn many<T, F: Parser<T, I>>(&mut self, f: F) -> Vec<T> {
		let mut results = vec![];
		while let Ok((v, r)) = f(self.rest.clone()) {
			self.rest = r;
			results.push(v);
		}

		results
	}
}

