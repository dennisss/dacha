
extern crate protobuf;

use protobuf::tokenizer::{Token, Tokenizer};
use protobuf::syntax::{proto, syntax};
use protobuf::compiler::Compiler;
use protobuf::spec::*;
use std::io::Write;

/*
	Will need a build entry point that we can use to generate all of the files



*/

fn main() -> std::io::Result<()> {


	let src = std::fs::read_to_string("testdata/message.proto")?;
	let mut outfile = std::fs::File::create("testdata/message.proto.rs")?;

	let mut tokenizer = Tokenizer::new(&src);
	let mut tokens = vec![];
	while let Some(tok) = tokenizer.next() {
		println!("{:?}", tok);
		match tok {
			Token::Whitespace => {},
			Token::Comment => {},
			_ => tokens.push(tok)
		};
	}

	let (desc, rest) = match proto(&tokens) {
		Ok(d) => d,
		Err(e) => {
			println!("{:?}", e);
			return Ok(());
		}
	};

	println!("{:?}", desc);

	if rest.len() != 0 {
		println!("Not parsed till end! {:?}", rest);
		return Ok(());
	}

	let outstr = Compiler::compile(&desc);

	outfile.write_all(outstr.as_bytes())?;
	outfile.flush()?;

	/*
	let s = "syntax = \"proto2\"; message A { required int b = 3 [default = \"sdfsdf\"]; }";
	
	let p = proto(&tokens); 
	println!("{:?}", p);
	*/

	Ok(())
}