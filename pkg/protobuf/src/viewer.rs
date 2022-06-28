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
                println!("{}varint {} => {}", indent, field.field_number, v);
            }
            WireValue::LengthDelim(data) => {
                let string = std::str::from_utf8(&data);

                if let Ok(s) = string {
                    let mut all_ascii_visible = true;
                    for c in s.chars() {
                        if (c as u32) < 32 || (c as u32) > 128 {
                            all_ascii_visible = false;
                            break;
                        }
                    }

                    // TODO: Must not just be ascii but visible ascii.
                    if all_ascii_visible {
                        // TODO: Escape the string being printed with the Bytes framework.
                        println!("{}string {} => (len: {}) \"{}\"", indent, field.field_number, s.len(), s);
                        continue;
                    }
                }

                // TODO: Consider trying this first?
                let next_indent = indent.to_owned() + "\t";
                println!("{}message {} =>", indent, field.field_number);
                match print_message(data, &next_indent) {
                    Ok(()) => {}
                    Err(e) => {
                        // NOTE: We assume that print_message will only fail if nothing was
                        // printed.
                        println!(
                            "{}unknown {} => {:?}",
                            indent,
                            field.field_number,
                            Bytes::from(data)
                        );
                    }
                }
            }
            WireValue::Word32(v) => {
                println!("{}word32 {} => {:x?}", indent, field.field_number, v);
            }
            WireValue::Word64(v) => {
                println!("{}word64 {} => {:x?}", indent, field.field_number, v);
            }
            _ => println!("{}unimplemented {} => ?", indent, field.field_number),
        }
    }

    Ok(())
}
