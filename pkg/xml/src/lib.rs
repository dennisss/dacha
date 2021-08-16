pub mod spec;
mod syntax;

use common::errors::*;
pub use spec::*;

fn pretty_print_parsing_error(input: &str, remaining_bytes: usize) {
    let target_i = input.len() - remaining_bytes;

    let mut line_number = 1;
    let mut line_start = 0;

    let mut line_end = None;

    for (idx, c) in input.char_indices() {
        if c == '\n' {
            let i = idx + 1; // NOTE: This assumes that '\n' is a 1 byte character.
            if i > target_i {
                line_end = Some(i);
                break;
            } else {
                line_start = i;
                line_number += 1;
            }
        }
    }

    let line_end = line_end.unwrap_or(input.len());

    println!("Line {:4}: {}", line_number, &input[line_start..line_end]);

    let mut pointer = String::from("           ");

    // TODO: Verify that this will never overflow.
    for i in 0..(target_i - line_start) {
        pointer.push(' ');
    }

    pointer.push('^');

    println!("{}", pointer);
}

pub fn parse(input: &str) -> Result<Document> {
    let res = parsing::complete(syntax::parse_document)(input);
    match res {
        Ok((doc, _)) => Ok(doc),
        Err(e) => {
            if let Some(parsing::ParserError {
                remaining_bytes, ..
            }) = e.downcast_ref()
            {
                pretty_print_parsing_error(input, *remaining_bytes);
            }

            return Err(e);
        }
    }
}
