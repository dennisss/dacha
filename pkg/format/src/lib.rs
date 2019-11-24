#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate nom;
#[macro_use] extern crate error_chain;

extern crate math;
extern crate byteorder;
extern crate num_traits;

pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
		}
	}
}

pub mod image;
// pub mod protobuf;

