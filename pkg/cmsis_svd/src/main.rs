#[macro_use]
extern crate common;
extern crate cmsis_svd;
extern crate xml;

use std::io::Write;

use automata::regexp::vm::instance::RegExp;
use cmsis_svd::compiler::*;
use common::errors::*;
use common::line_builder::LineBuilder;

/*
Naming Conventions:
- Each peripheral named 'PERIPHERAL_NAME'
    - Will be stored in a module named 'peripheral_name'
    - In that module will have a struct containing all the registers named 'PERIPHERAL_NAME'
- Each register named 'REGISTER_NAME'
    - Will have a sub-module at 'peripheral_name::register_name'
    - In that module, it will have a struct named 'REGISTER_NAME'
- Reading from the 'REGISTER_NAME' struct produces a 'REGISTER_NAME_[READ_]VALUE'
- Writing to the 'REGISTER_NAME' struct requires making a 'REGISTER_NAME_[WRITE_]VALUE' struct
- The above 'READ_' and 'WRITE_' suffixes are skipped if the same type can be used to represent both.
- Fields use structs 'FIELD_NAME_[READ_]FIELD' and 'FIELD_NAME_[WRITE_]FIELD'

Generated code:

// Each peripheral will generate a top level module
pub mod peripheral_name {
    pub struct PERIPHERAL_NAME {}

    impl PERIPHERAL_NAME {
        #[inline(always)]
        pub fn base_address() -> u32 {  }

        pub fn register_name<'a>(&'a self) -> &'a REGISTER_NAME;
    }

    pub struct REGISTER_NAME {

    }

    pub struct REGISTER_NAMERead {

    }

    pub mod register_name {
        // Register
        pub struct Register {}

        impl Register {
            pub fn read(&self) -> ReadValue;
            pub fn write(&mut self, value: WriteValue);
        }

        pub struct ReadValue {

        }

        pub struct WriteValue {

        }
    }
}

Given a path to a

*/

/*
For registers, the 'size' will be inherited from device, peripherals, or elements:
- https://siliconlabs.github.io/Gecko_SDK_Doc/CMSIS/SVD/html/group__register_properties_group__gr.html
*/

/*
We want to rewrite all fields with register_name: "EVENT_.*" to use an EventState struct

Currently we're at 119479 generated lined
                   114205
                   113929
                   100894
                   294471
                   292707
                   258524
                   226812
*/

/*

TODO: Must support the <access> element directly on <field> elements.

On a <peripheral>:
- Attribute: "derivedFrom"
- Child: "baseAddress":  0x10001000
- "registers"


Each peripheral is a struct

Each register is also a struct

Each register field may also end up being a struct

peripherals::clock().start().set_


Interesting usage of clusters:


*/

fn main() -> Result<()> {
    Ok(())
}

/*
TODO: Implement alternatePeripheral
    - Two peripherals are only allowed to share the same address space if using this field

Needed features:
- Deduplicating redunant enums (e.g. PIN0_WRITE has High and Low)
    - Also for TASK and EVENT
- Vectorize the GPIO registers?
*/

// read() -> SOMETHING_READ
// write(value: SOMETHING_WRITE)

// TXPOWER_READ_VALUE
// TXPOWER_WRITE_VALUE
// TXPOWER_READ_FIELD

/*
TODO: Assuming
*/
