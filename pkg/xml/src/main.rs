extern crate common;
extern crate xml;

use common::errors::*;

fn main() -> Result<()> {
    let input = std::fs::read_to_string("/home/dennis/workspace/dacha/third_party/nordic/nrf52840.svd")?;
    let doc = xml::parse(&input)?;

    println!("{}", doc.root_element.name);


    Ok(())
}