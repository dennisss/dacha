#![feature(trait_alias, core_intrinsics, str_internals)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate failure;
extern crate reflection;

use common::bytes::Buf;
use common::bytes::Bytes;
use common::errors::*;

#[derive(Debug, PartialEq)]
pub enum ParserErrorKind {
    Incomplete,
    UnexpectedValue,
}

#[derive(Debug, Fail)]
pub struct ParserError {
    pub kind: ParserErrorKind,
    pub message: String,
    pub remaining_bytes: usize,
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(self, f)
    }
}

pub fn incomplete_error(remaining_bytes: usize) -> Error {
    ParserError {
        kind: ParserErrorKind::Incomplete,
        message: "Incomplete".into(),
        remaining_bytes,
    }
    .into()
}

pub fn is_incomplete(e: &Error) -> bool {
    if let Some(ParserError { kind, .. }) = e.downcast_ref() {
        *kind == ParserErrorKind::Incomplete
    } else {
        false
    }
}

pub mod ascii;
pub mod binary;
pub mod cstruct;
pub mod iso;
pub mod opaque;

//
//pub trait CharIter {
//	fn take_char(self) -> Option<(char, Self)>;
//}
//
//impl<'a> CharIter for &'a str {
//	fn take_char(self) -> Option<(char, Self)> {
//		let mut chars = self.chars();
//		chars.next().map(|c| (c, chars.as_str()))
//	}
//}
//
//// TODO: Can be generic for any AsRef?
//impl<'a> CharIter for Bytes {
//	fn take_char(mut self) -> Option<(char, Self)> {
//		let first = match self.get(0) {
//			Some(v) => *v,
//			None => { return None; }
//		};
//
//		let size = core::str::utf8_char_width(first);
//		if self.len() < size {
//			return None;
//		}
//
//		// TODO: Check that this won't fail.
//		let c = std::str::from_utf8(&self[0..size]).unwrap().chars().next().unwrap();
//		self.advance(size);
//
//		Some((c, self))
//	}
//}

//pub trait CharIter {
//	fn char_iter(&self) -> std::str::Chars;
//}
//
//impl CharIter for &str {
//	fn char_iter(&self) -> Chars {
//		self.chars()
//	}
//}

// TODO: Implement into ParserFeed
pub trait IsOneOf<S: ?Sized> {
    fn is_one_of(self, s: &'static S) -> bool;
}

impl IsOneOf<str> for char {
    fn is_one_of(self, s: &'static str) -> bool {
        s.chars().find(|x| *x == self).is_some()
    }
}

impl<T: AsRef<[u8]> + ?Sized> IsOneOf<T> for u8 {
    fn is_one_of(self, s: &'static T) -> bool {
        s.as_ref().iter().find(|x| **x == self).is_some()
    }
}

//pub fn is_one_of(s: &'static str, input: char) -> bool {
//	s.chars().find(|x| *x == input).is_some()
//}

// NOTE: This will parse the input as utf-8
pub fn one_of<S: ?Sized, T: Copy + IsOneOf<S>, I: ParserFeed>(s: &'static S) -> impl Parser<T, I>
where
    I: ParserFeed<Item = T>,
{
    like(move |i: I::Item| i.is_one_of(s))
}

pub fn not_one_of<S: ?Sized, T: Copy + IsOneOf<S>, I: ParserFeed>(
    s: &'static S,
) -> impl Parser<T, I>
where
    I: ParserFeed<Item = T, Slice = S>,
{
    like(move |i: I::Item| !i.is_one_of(s))
}

pub fn atom<I, T: Copy + PartialEq>(v: T) -> impl Parser<T, I>
where
    I: ParserFeed<Item = T>,
{
    move |mut input: I| {
        let remaining_bytes = input.remaining_bytes();

        let item = input.next().ok_or(incomplete_error(remaining_bytes))?;
        if v == item {
            Ok((item, input))
        } else {
            Err(ParserError {
                kind: ParserErrorKind::UnexpectedValue,
                message: "Wrong atom in input".into(),
                remaining_bytes,
            }
            .into())
        }
    }
}

pub fn is<I, T, Y: PartialEq<T>, P: Parser<T, I>>(p: P, v: Y) -> impl Parser<T, I>
where
    I: ParserFeed,
{
    move |input: I| {
        let remaining_bytes = input.remaining_bytes();
        let (value, rest) = p(input)?;
        if v == value {
            Ok((value, rest))
        } else {
            Err(ParserError {
                kind: ParserErrorKind::UnexpectedValue,
                message: "Wrong value of atom".into(),
                remaining_bytes,
            }
            .into())
        }
    }
}

pub type ParseError = Error;
pub type ParserInput = ::common::bytes::Bytes;
pub type ParseResult<T, I = ParserInput> = ParseResult2<T, I>;
pub type ParseResult2<T, R> = std::result::Result<(T, R), ParseError>;
pub trait Parser<T, I = ParserInput> = Fn(I) -> ParseResult<T, I>;

pub trait ParserFeed: Clone {
    type Item: Copy;
    type Slice: ?Sized; // A non-owned slice.

    fn next(&mut self) -> Option<Self::Item>;

    fn empty(&self) -> bool;

    fn remaining_bytes(&self) -> usize;

    fn take_while<F: Fn(Self::Item) -> bool>(self, f: F) -> (Self, Self);

    /// Given that self runs over the range [0, N) and other runs over the
    /// range [K, N) in the same underlying buffer, this should return a new
    /// slice with range [0, K).
    ///
    /// NOTE: Its very unsafe to use this unless you know where you got your
    /// buffers from.
    fn slice_before(self, other: Self) -> (Self, Self);

    fn strip_prefix(&mut self, prefix: &Self::Slice) -> bool;

    fn take_exact(self, n: usize) -> Option<(Self, Self)>;
}

impl ParserFeed for &[u8] {
    type Item = u8;
    type Slice = [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.split_first().map(|(v, rest)| {
            *self = rest;
            *v
        })
    }

    fn empty(&self) -> bool {
        self.len() == 0
    }
    fn remaining_bytes(&self) -> usize {
        self.len()
    }

    fn take_while<F: Fn(Self::Item) -> bool>(self, f: F) -> (Self, Self) {
        let mut rest = self;
        while let Some((v, r)) = self.split_first() {
            if f(*v) {
                rest = r;
            } else {
                break;
            }
        }

        self.slice_before(rest)
    }

    fn slice_before(self, other: Self) -> (Self, Self) {
        let mid = self.len() - other.len();
        self.split_at(mid)
    }

    fn strip_prefix(&mut self, prefix: &Self::Slice) -> bool {
        if self.starts_with(prefix) {
            *self = &self[prefix.len()..];
            true
        } else {
            false
        }
    }

    fn take_exact(self, n: usize) -> Option<(Self, Self)> {
        if self.len() < n {
            None
        } else {
            Some(self.split_at(n))
        }
    }
}

impl ParserFeed for ::common::bytes::Bytes {
    type Item = u8;
    type Slice = [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.empty() {
            return None;
        }

        let v = self[0];
        self.advance(1);
        Some(v)
    }

    fn empty(&self) -> bool {
        self.len() == 0
    }

    fn remaining_bytes(&self) -> usize {
        self.len()
    }

    fn take_while<F: Fn(Self::Item) -> bool>(mut self, f: F) -> (Self, Self) {
        let mut i = 0;
        while i < self.len() {
            if !f(self[i]) {
                break;
            }
            i += 1;
        }

        let rest = self.split_off(i);
        (self, rest)
    }

    fn slice_before(self, other: Self) -> (Self, Self) {
        (self.slice(0..(self.len() - other.len())), other)
    }

    fn strip_prefix(&mut self, prefix: &Self::Slice) -> bool {
        if self.starts_with(prefix) {
            self.advance(prefix.len());
            true
        } else {
            false
        }
    }

    fn take_exact(mut self, n: usize) -> Option<(Self, Self)> {
        if self.len() < n {
            return None;
        }

        let rest = self.split_off(n);
        Some((self, rest))
    }
}

impl ParserFeed for &str {
    type Item = char;
    type Slice = str;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: Not needed as chars returns an Option?
        if self.empty() {
            return None;
        }

        let mut chars = self.chars();
        let c = chars.next();
        *self = chars.as_str();
        c
    }

    fn empty(&self) -> bool {
        self.len() == 0
    }

    fn remaining_bytes(&self) -> usize {
        self.len()
    }

    fn take_while<F: Fn(Self::Item) -> bool>(self, f: F) -> (Self, Self) {
        let mut rest = self;
        let mut chars = self.chars();
        while let Some(c) = chars.next() {
            if f(c) {
                rest = chars.as_str();
            } else {
                break;
            }
        }

        self.slice_before(rest)
    }

    fn slice_before(self, other: Self) -> (Self, Self) {
        let mid = self.len() - other.len();
        self.split_at(mid)
    }

    fn strip_prefix(&mut self, prefix: &Self::Slice) -> bool {
        if self.starts_with(prefix) {
            *self = &self[prefix.len()..];
            true
        } else {
            false
        }
    }

    fn take_exact(self, n: usize) -> Option<(Self, Self)> {
        // TODO: This is wrong. We may find the actual byte offset.
        if self.len() < n {
            return None;
        }

        Some(self.split_at(n))
    }
}

#[macro_export]
macro_rules! alt {
	( $first:expr, $( $next:expr ),* ) => {
		move |input| {
			let mut errs = vec![];
            let mut max_remaining = std::usize::MAX;

			match ($first)(::std::clone::Clone::clone(&input)) {
				Ok(v) => { return Ok(v); },
				Err(e) => {
                    if let Some(ParserError { remaining_bytes, .. }) = e.downcast_ref() {
                        max_remaining = std::cmp::min(max_remaining, *remaining_bytes);
                    }

                    errs.push(e.to_string());
                }
			};

			$(
			match ($next)(::std::clone::Clone::clone(&input)) {
				Ok(v) => { return Ok(v); },
				Err(e) => {
                    if let Some(ParserError { remaining_bytes, .. }) = e.downcast_ref() {
                        max_remaining = std::cmp::min(max_remaining, *remaining_bytes);
                    }

                    errs.push(e.to_string());
                }
			};
			)*

			// TODO: Must support Incomplete error in some reasonable way.
            Err(Error::from(ParserError {
                kind: ParserErrorKind::UnexpectedValue,
                message: format!("({})", errs.join(" | ")),
                remaining_bytes: max_remaining
            }))
		}
	};
}

// TODO: Try to convert to a function taking a lambda with an &mut ParserCursor
// parameter
#[macro_export]
macro_rules! seq {
    ($c: ident => $e:expr) => {
        move |input| {
            let mut $c = ParseCursor::new(input);
            // Wrapped in a function so that the expression can have return statements.
            let mut f = || $e;
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
    }};
}

#[macro_export]
macro_rules! parser {
	(pub $c:ident<$t:ty> => $e:expr) => {
		parser!(pub $c<ParserInput, $t> => $e);
	};
    (pub(crate) $c:ident<$t:ty> => $e:expr) => {
		parser!(pub(crate) $c<ParserInput, $t> => $e);
	};
	($c:ident<$t:ty> => $e:expr) => {
		parser!($c<ParserInput, $t> => $e);
	};

	(pub $c:ident<$r:ty, $t:ty> => $e:expr) => {
		pub fn $c(input: $r) -> ParseResult2<$t, $r> {
			let p = $e;
			p(input)

			// TODO: Must support passing through Incomplete error
			//.map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
    (pub(crate) $c:ident<$r:ty, $t:ty> => $e:expr) => {
		pub(crate) fn $c(input: $r) -> ParseResult2<$t, $r> {
			let p = $e;
			p(input)

			// TODO: Must support passing through Incomplete error
			//.map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
	// Same thing as the first form, but not public.
	($c:ident<$r:ty, $t:ty> => $e:expr) => {
		fn $c(input: $r) -> ParseResult2<$t, $r> {
			let p = $e;
			p(input)
			// TODO: Must support passing through Incomplete error
			// .map_err(|e: Error| Error::from(format!("{}({})", function!(), e)))
		}
	};
}

/// Converts a parser of &[u8] to a parser of Bytes.
pub fn as_bytes<T: 'static, F>(f: F) -> impl Parser<T, Bytes>
where
    F: for<'a> Fn(&'a [u8]) -> Result<(T, &'a [u8])>,
{
    move |mut input: Bytes| {
        let (v, rest) = f(&input)?;
        input.advance(input.len() - rest.len());
        Ok((v, input))
    }
}

pub fn map<I, T, Y, P, F>(p: P, f: F) -> impl Fn(I) -> Result<(Y, I)>
where
    P: Fn(I) -> Result<(T, I)>,
    F: Fn(T) -> Y,
{
    move |input: I| p(input).map(|(v, rest)| (f(v), rest))
}

pub fn and_then<I, T, Y, P: Parser<T, I>, F: Fn(T) -> std::result::Result<Y, ParseError>>(
    p: P,
    f: F,
) -> impl Parser<Y, I> {
    move |input: I| p(input).and_then(|(v, rest)| Ok((f(v)?, rest)))
}

pub fn then<I, T, Y, P: Parser<T, I>, G: Parser<Y, I>>(p: P, g: G) -> impl Parser<Y, I> {
    move |input: I| p(input).and_then(|(_, rest)| g(rest))
}

pub fn like<I, T: Copy, F: Fn(T) -> bool>(f: F) -> impl Parser<T, I>
where
    I: ParserFeed<Item = T>,
{
    move |mut input: I| {
        let remaining_bytes = input.remaining_bytes();
        let item = input.next().ok_or(incomplete_error(remaining_bytes))?;

        if f(item) {
            Ok((item, input))
        } else {
            Err(err_msg("like failed"))
        }
    }
}

parser!(pub any<u8> => like(|_| true));

pub fn complete<I: ParserFeed, T, P: Parser<T, I>>(p: P) -> impl Parser<T, I> {
    move |input: I| {
        p(input)
            .and_then(|(v, rest)| {
                if !rest.empty() {
                    // Err(ParserError {
                    //     kind: ParserErrorKind::UnexpectedValue,

                    // })

                    Err(format_err!(
                        "Failed to parse last {} bytes",
                        rest.remaining_bytes()
                    ))
                } else {
                    Ok((v, rest))
                }
            })
            .map_err(|e| {
                if is_incomplete(&e) {
                    err_msg("Expected to be complete")
                } else {
                    e
                }
            })
    }
}

pub fn peek<I: Clone, T, P: Parser<T, I>>(p: P) -> impl Parser<(), I> {
    move |input: I| {
        if let Ok(_) = p(input.clone()) {
            Ok(((), input))
        } else {
            // TODO: Return the parser error?
            Err(err_msg("Peek failed"))
        }
    }
}

/// Takes a specified number of bytes from the input
pub fn take_exact<I: ParserFeed>(length: usize) -> impl Parser<I, I> {
    move |input: I| {
        let remaining_bytes = input.remaining_bytes();
        input
            .take_exact(length)
            .ok_or_else(|| incomplete_error(remaining_bytes))
    }
}

pub fn take_while<I: ParserFeed, T: Copy, F: Fn(T) -> bool>(f: F) -> impl Parser<I, I>
where
    I: ParserFeed<Item = T>,
{
    move |input: I| Ok(input.take_while(|v| f(v)))
}

pub fn take_while1<I: ParserFeed, T: Copy, F: Fn(T) -> bool>(f: F) -> impl Parser<I, I>
where
    I: ParserFeed<Item = T>,
{
    let p = take_while(f);
    move |input: I| {
        let (v, rest) = p(input)?;
        if !v.empty() {
            Ok((v, rest))
        } else {
            Err(err_msg("take_while1 failed"))
        }
    }
}

/// Continue parsing until the given parser is able to parse
/// NOTE: The parser must parse a non-empty pattern for this to work.
///
/// NOTE: Unless the given parser only consumes a few bytes on average, this
/// will be slow.
pub fn take_until<I: ParserFeed, T, P: Parser<T, I>>(p: P) -> impl Parser<I, I> {
    move |mut input: I| {
        let mut rem = input.clone();
        while !rem.empty() {
            match p(rem.clone()) {
                Ok(_) => {
                    return Ok(input.slice_before(rem));
                }
                // Advance by one so that next time the parser tries parsing
                // from a different position.
                Err(_) => {
                    rem.next();
                }
            }
        }

        // TODO: Return Incomplete error.
        Err(ParserError {
            kind: ParserErrorKind::UnexpectedValue,
            message: "Hit end of input before seeing pattern".into(),
            remaining_bytes: input.remaining_bytes(),
        }
        .into())
    }
}

//
// - THis is basically a split prefix
// TODO: Implement a binary version for efficient binary ASCII string parsing.
// (and refactor most ussages to use that)
pub fn tag<I, S: ?Sized, T: AsRef<S> + std::fmt::Debug>(s: T) -> impl Parser<(), I>
where
    I: ParserFeed<Slice = S>,
{
    move |mut input: I| {
        let t = s.as_ref();

        // TODO: Improve error returns in this function.
        // ^ Probably need to leverage an Incomplete error
        if input.strip_prefix(t) {
            Ok(((), input))
        } else {
            Err(ParserError {
                kind: ParserErrorKind::UnexpectedValue,
                message: format!("Expected \"{:?}\"", s),
                remaining_bytes: input.remaining_bytes(),
            }
            .into())
        }

        /*
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
        */
    }
}

pub fn anytag<I, S: ?Sized, T: AsRef<S> + std::fmt::Debug>(arr: &'static [T]) -> impl Parser<I, I>
where
    I: ParserFeed<Slice = S>,
{
    move |input: I| {
        for t in arr.iter() {
            // TODO: This can be expensive because of all the copying needed
            if let Ok(v) = slice(tag(t))(input.clone()) {
                return Ok(v);
            }
        }

        // println!("{:?}", std::str::from_utf8(
        // 	&input[0..std::cmp::min(40, input.len())].as_ref()).unwrap());

        // TODO: Should we convert to an Incomplete error in some cases?
        Err(ParserError {
            kind: ParserErrorKind::UnexpectedValue,
            message: "No matching tag".into(),
            remaining_bytes: input.remaining_bytes(),
        }
        .into())
    }
}

pub fn opt<I: Clone, T, P: Parser<T, I>>(p: P) -> impl Parser<Option<T>, I> {
    move |input: I| match p(input.clone()) {
        Ok((v, rest)) => Ok((Some(v), rest)),
        Err(_) => Ok((None, input)),
    }
}

pub fn many<I: Clone, T, P: Parser<T, I>>(p: P) -> impl Parser<Vec<T>, I> {
    move |input: I| {
        let mut results = vec![];
        let mut rest = input.clone();
        while let Ok((v, r)) = p(rest.clone()) {
            rest = r;
            results.push(v);
        }

        Ok((results, rest))
    }
}

pub fn many1<I: Clone, T, F: Parser<T, I>>(f: F) -> impl Parser<Vec<T>, I> {
    let p = many(f);
    move |input: I| {
        let (vec, rest) = p(input)?;
        if vec.len() < 1 {
            return Err(err_msg("many1 failed"));
        }

        Ok((vec, rest))
    }
}

/// Parses zero or more items delimited by another parser.
/// e.g. 'delimited(tag("a"), tag(","))' will match comma separated 'a's
pub fn delimited<I: Clone, T, Y, P: Parser<T, I>, D: Parser<Y, I>>(
    p: P,
    d: D,
) -> impl Parser<Vec<T>, I> {
    move |mut input| {
        let mut out = vec![];

        // Match first value.
        match p(std::clone::Clone::clone(&input)) {
            Ok((v, rest)) => {
                out.push(v);
                input = rest;
            }
            Err(e) => {
                return Ok((out, input));
            }
        };

        loop {
            // Parse delimiter (but don't update 'input' yet).
            let rest = match d(std::clone::Clone::clone(&input)) {
                Ok((_, rest)) => rest,
                Err(_) => {
                    return Ok((out, input));
                }
            };

            match p(rest) {
                Ok((v, rest)) => {
                    out.push(v);
                    input = rest;
                }
                // On failure, return before the delimiter was parsed
                Err(_) => {
                    return Ok((out, input));
                }
            }
        }
    }
}

pub fn delimited1<I: Clone, T, Y, P: Parser<T, I>, D: Parser<Y, I>>(
    p: P,
    d: D,
) -> impl Parser<Vec<T>, I> {
    and_then(delimited(p, d), |arr| {
        if arr.len() < 1 {
            Err(err_msg("No items parseable"))
        } else {
            Ok(arr)
        }
    })
}

// TODO: Ensure that this doesn't get applied to ranges that do things like
// decoding of escaped characters.
/// Given a parser, output a parser which outputs the entire range of the input
/// that is consumed by the parser if successful.
pub fn slice<I: ParserFeed, T, P: Parser<T, I>>(p: P) -> impl Parser<I, I> {
    move |input: I| {
        let (_, rest) = p(input.clone())?;
        Ok(input.slice_before(rest))
    }
}

pub fn slice_with<T, P: Parser<T>>(p: P) -> impl Parser<(T, Bytes)> {
    move |input: Bytes| {
        let (v, rest) = p(input.clone())?;
        let n = input.len() - rest.len();
        Ok(((v, input.slice(0..n)), rest))
    }
}

#[derive(Clone)]
pub struct ParseCursor<I: Clone = ParserInput> {
    rest: I,
}

impl<I: Clone> ParseCursor<I> {
    pub fn new(input: I) -> Self {
        Self {
            rest: input.clone(),
        }
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
            }
            Err(e) => Err(e),
        }
    }

    // TODO: Deprecate in favor of the separate 'is' function?
    //	pub fn is<T: PartialEq<Y>, Y, F: Parser<T, I>>(&mut self, f: F, v: Y)
    //		-> std::result::Result<T, ParseError> {
    //		match f(self.rest.clone()) {
    //			Ok((v2, r)) => {
    //				if v2 != v {
    //					return Err(err_msg("is failed"));
    //				}
    //
    //				self.rest = r;
    //				Ok(v2)
    //			},
    //			Err(e) => Err(e)
    //		}
    //	}

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

impl ParseCursor<Bytes> {
    pub fn nexts<T, F: Fn(&[u8]) -> std::result::Result<(T, &[u8]), ParseError>>(
        &mut self,
        f: F,
    ) -> std::result::Result<T, ParseError> {
        match f(&self.rest) {
            Ok((v, r)) => {
                let n = self.rest.len() - r.len();
                self.rest.advance(n);
                Ok(v)
            }
            Err(e) => Err(e),
        }
    }
}

#[macro_export]
macro_rules! parse_next {
    ($input:expr, $f:expr) => {{
        let (v, rest) = $f($input)?;
        $input = rest;
        v
    }};

    ($input:expr, $f:expr, $( $arg:expr ),*) => {{
        let (v, rest) = $f($input, $($arg),*)?;
        $input = rest;
        v
    }};
}
