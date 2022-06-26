use std::borrow::ToOwned;

use common::bytes::Bytes;
use common::errors::*;

use crate::wire::WireField;
use crate::wire::WireValue;

pub fn print_message(data: &[u8], indent: &str) -> Result<()> {
    let fields = WireField::parse_all(data)?;

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
