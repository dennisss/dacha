extern crate protobuf;

use common::errors::*;
use protobuf::text::parse_text_proto;
use std::io::Write;

/*
    Simple algorithm for building:
    - Effectively need a fuse filesystem that overlays autogenerated files with the regular src files
      but that needs to work well with rls



    Will need a build entry point that we can use to generate all of the files



*/

const SAMPLE_TEXTPROTO: &'static str = "hello: WORLD apples: [1,2, 3] world < a: 2 >";

use protobuf::wire::WireValue;

use common::bytes::Bytes;

fn print_message(data: &[u8], indent: &str) -> Result<()> {
    let fields = protobuf::wire::WireField::parse_all(data)?;

    // println!("{:?}", &fields);

    // NOTE: As a heuristic we could detect between a string and bytes by whether or
    // not it is utf8
    for field in &fields {
        match field.value {
            WireValue::Varint(v) => {
                println!("{}{} => {}", indent, field.field_number, v);
            }
            WireValue::LengthDelim(data) => {
                let string = std::str::from_utf8(&data);
                match string {
                    Ok(s) => {
                        println!("{}{} => \"{}\"", indent, field.field_number, s);
                    }
                    Err(e) => {
                        let next_indent = indent.to_owned() + "\t";
                        println!("{}{} =>", indent, field.field_number);
                        match print_message(data, &next_indent) {
                            Ok(()) => {}
                            Err(e) => {
                                // NOTE: We assume that print_message will only fail if nothing was
                                // printed.
                                println!(
                                    "{}{} => {:?}",
                                    indent,
                                    field.field_number,
                                    Bytes::from(data)
                                );
                            }
                        }
                    }
                }
            }
            _ => println!("{}{} => ?", indent, field.field_number),
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    // Repin
    /*
    let data =
    common::base64::decode("CAsQAhoaVWd6XzQzWmQtRTFDU1R4VldUQjRBYUFCQWcqCzlBdGhsWHVVU1RjMABKFTEwNjA4NTUwNDI4ODM5NTk1MjMyOVABqAEMugEYVUNzUS00LWlMTHVNMUNjeE15VVJKM2xB")?;

     */

    /*
    // Unpin
    let data =
    common::base64::decode_config("CAsQAhoaVWd4OHM5UVIzZjVUY0d3SjNycDRBYUFCQWcqC1lRT0x2VVNRendZMABKFTExMzc5NjA4MDQ0NzI2MDk0NTAxNVABigEmEgtZUU9MdlVTUXp3WcABAMgBAOABA6ICDSj___________8BQACoAQy6ARhVQ3IyMnhpa1dVSzJ5VVc0WXhPS1hjbFE=", base64::URL_SAFE)?;

    print_message(&data, "")?;

    return Ok(());
    */

    let v = protobuf::text::parse_text_syntax(SAMPLE_TEXTPROTO)?;
    println!("{:#?}", v);
    return Ok(());

    //	let src = std::fs::read_to_string("testdata/message.proto")?;
    //	let mut outfile = std::fs::File::create("testdata/message.proto.rs")?;

    /*
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
    */

    /*
    let s = "syntax = \"proto2\"; message A { required int b = 3 [default = \"sdfsdf\"]; }";

    let p = proto(&tokens);
    println!("{:?}", p);
    */

    Ok(())
}
