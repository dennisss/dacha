#![feature(let_chains)]

extern crate common;
extern crate parsing;
#[macro_use]
extern crate regexp_macros;
extern crate automata;

use common::errors::*;

mod block;
mod block_builder;
mod encoding;
mod inline;
mod inline_parser;

pub use block::*;
pub use inline::*;

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() {
        /*
        println!("{:#?}", Block::parse_document("hello world"));

        println!("{:#?}", Block::parse_document("hello\nworld"));

        println!("{:#?}", Block::parse_document("hello\n\nworld"));

        println!("{:#?}", Block::parse_document("# hello\n\nworld"));

        println!("{:#?}", Block::parse_document("**hello *world***"));
        */

        println!(
            "{:#?}",
            Block::parse_document("```\n<\n >\n```\n") // .to_html()
        );
    }
}

/*
=

*/

/*
Example:
    ``
    foo
    bar
    baz
    ``
is a single paragraph with a single code span in it

*/
