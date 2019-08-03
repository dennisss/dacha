
extern crate protobuf;

use protobuf::tokenizer::{Token, Tokenizer};
use protobuf::syntax2::{proto, syntax};

fn main() {

	let s = "syntax = \"proto2\"; message A { required int b = 3 [default = \"sdfsdf\"]; }";

	let mut t = Tokenizer::new(&s);
	let mut tokens = vec![];
	while let Some(tok) = t.next() {
		match tok {
			Token::Whitespace => continue,
			Token::Comment => continue,
			_ => tokens.push(tok)
		};
	}
	
	let p = proto(&tokens); 
	println!("{:?}", p);

}