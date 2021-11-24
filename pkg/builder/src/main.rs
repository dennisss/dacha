extern crate builder;
extern crate common;

use common::errors::*;

fn main() -> Result<()> {
    builder::cli::run()
}
