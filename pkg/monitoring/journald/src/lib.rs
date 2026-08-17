#[macro_use]
extern crate sys;
#[macro_use]
extern crate base_util;
#[macro_use]
extern crate async_trait;
extern crate alloc;

pub mod bindings {
    //! Bindgen produced bindings.

    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(unused)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

