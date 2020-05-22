
extern crate protobuf;

use common::errors::*;
use protobuf::tokenizer::{Token};
use protobuf::syntax::{proto, syntax};
use protobuf::compiler::Compiler;
use protobuf::text::parse_text_proto;
use protobuf::spec::*;
use std::io::Write;

/*
	Will need a build entry point that we can use to generate all of the files



*/

const SAMPLE_TEXTPROTO: &'static str = "hello: WORLD apples: [1,2, 3]";

fn main() -> Result<()> {

	/*
	let v = parse_text_proto(SAMPLE_TEXTPROTO)?;
	return Ok(());
	*/

//	let src = std::fs::read_to_string("testdata/message.proto")?;
//	let mut outfile = std::fs::File::create("testdata/message.proto.rs")?;

	let src = std::fs::read_to_string("pkg/rpc/src/proto/adder.proto")?;
	let mut outfile = std::fs::File::create("pkg/rpc/src/proto/adder.rs")?;

	let (desc, rest) = match proto(&src) {
		Ok(d) => d,
		Err(e) => {
			println!("{:?}", e);
			return Ok(());
		}
	};

	println!("{:#?}", desc);

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