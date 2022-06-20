extern crate asn;
#[macro_use]
extern crate common;

use std::path::Path;

use common::errors::*;

fn main() -> Result<()> {
    asn::build_in_directory(
        &project_path!("third_party/pkix"),
        Path::new("/tmp/asn_compile"),
    )
}
