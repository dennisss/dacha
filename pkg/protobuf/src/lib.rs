#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators, trait_alias)]

#[macro_use] extern crate nom;
#[macro_use] extern crate error_chain;

extern crate math;
extern crate byteorder;
extern crate num_traits;

pub mod spec;
pub mod tokenizer;
pub mod syntax2;

