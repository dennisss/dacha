

#[macro_use]
extern crate macros;

use common::errors::*;
use kicad::library::*;
use kicad::reader::*;
use kicad::serializer::*;
use reflection::ParseFrom;
use reflection::SerializeTo;

// /home/dennis/.config/kicad/7.0/sym-lib-table
/*

 fp_lib_table

(sym_lib_table
  (version 7)
  (lib (name "4xxx")(type "KiCad")(uri "..path..")(options "")(descr "4xxx series symbols"))
  ...
)

*/


//         


#[executor_main]
async fn main() -> Result<()> {

    let data = file::read_to_string("/home/dennis/.config/kicad/7.0/sym-lib-table").await?;

    let e = kicad::sexpr::SExpr::parse(&data)?;

    let input = SExprReader::new(&e);

    let object = LibraryTable::parse_from(input)?;

    let mut out = SExprSerializer::new();
    object.serialize_to(out.root("sym_lib_table")?)?;



    println!("{}", out.finish());

    Ok(())
}