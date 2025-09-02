
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::process::Command;
use std::io::Write;
use std::collections::HashSet;

use common::errors::*;
use kicad::library::*;
use kicad::reader::*;
use kicad::serializer::*;
use kicad::export::*;
use file::{LocalPath, LocalPathBuf};

#[derive(Args)]
struct Args {
    board_path: LocalPathBuf,
    output_dir: LocalPathBuf
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let export = KicadPCBExport::generate(&args.board_path, &args.output_dir)?;

    println!("{:#?}", export);

    Ok(())
}