extern crate asn;
#[macro_use]
extern crate common;

use file::LocalPath;

use common::errors::*;

fn main() -> Result<()> {
    asn::build_in_directory(
        &project_path!("third_party/pkix"),
        LocalPath::new("/tmp/asn_compile"),
    )
}
