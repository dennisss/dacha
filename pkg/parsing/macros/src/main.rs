use parsing::BinaryRepr;

#[macro_use]
extern crate parsing_macros;

#[bin(endian = "little")]
struct BasicFields {
    a: u8,
    b: u16,
    c: u32,
}

fn main() {
    assert_eq!(BasicFields::SIZE_OF, Some(7));

    println!("Hello world");
}

/*
bit

THe general type would

*/

// enum_def_with_
