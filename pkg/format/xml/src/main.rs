extern crate common;
extern crate xml;

use common::errors::*;

fn main() -> Result<()> {
    let input =
        std::fs::read_to_string(file::project_dir().join("third_party/nordic/nrf52840.svd"))?;
    let doc = xml::parse(&input)?;

    for node in &doc.root_element.content {
        match node {
            xml::Node::Element(e) => {
                println!("{:?}", e.name);

                if e.name == "licenseText" {
                    println!("{:?}", e.content);
                    break;
                }
            }
            xml::Node::Text(t) => {
                println!("TEXT: {:?}", t);
            }
            _ => {}
        }
    }

    println!("{}", doc.root_element.name);

    Ok(())
}
