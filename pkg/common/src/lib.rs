#![feature(
    trait_alias,
    associated_type_defaults,
    specialization,
    const_fn_trait_bound,
    try_trait_v2,
    const_slice_from_raw_parts,
    maybe_uninit_slice,
    slice_take,
    allocator_api,
    slice_ptr_get,
    core_intrinsics
)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate async_trait;
#[cfg(feature = "std")]
#[macro_use]
pub extern crate failure;
#[macro_use]
extern crate arrayref;
#[cfg(feature = "std")]
pub extern crate async_std;
#[cfg(feature = "std")]
pub extern crate base64;
#[cfg(feature = "std")]
pub extern crate bytes;
#[cfg(feature = "std")]
pub extern crate futures;
#[cfg(feature = "std")]
pub extern crate libc;
#[cfg(feature = "std")]
#[macro_use]
pub extern crate lazy_static;
#[cfg(feature = "std")]
pub extern crate chrono;
pub extern crate generic_array;
#[cfg(target_os = "linux")]
pub extern crate nix;
pub extern crate typenum;

#[macro_use]
extern crate macros;

#[cfg(feature = "std")]
pub extern crate base_args as args;

#[cfg(feature = "std")]
pub mod algorithms;
#[cfg(feature = "alloc")]
pub mod aligned;
pub mod any;
#[cfg(feature = "std")]
pub mod async_fn;
pub mod attribute;
#[cfg(feature = "std")]
pub mod bit_set;
#[cfg(feature = "std")]
pub mod bits;
#[cfg(feature = "std")]
pub mod borrowed;
#[cfg(feature = "std")]
pub mod buffered_reader;
pub mod collections;
pub mod concat_slice;
pub mod const_default;
pub mod cyclic_buffer;
#[cfg(feature = "std")]
pub mod factory;
pub mod fixed;
pub mod hash;
#[cfg(feature = "std")]
pub mod io;
#[cfg(feature = "std")]
pub mod line_builder;
pub mod list;
#[cfg(feature = "std")]
pub mod loops;
pub mod option;
#[cfg(feature = "std")]
pub mod pipe;
pub mod register;
pub mod segmented_buffer;
#[cfg(feature = "alloc")]
pub mod small;
pub mod sort;
pub mod struct_bytes;
#[cfg(feature = "alloc")]
pub mod tree;
#[cfg(feature = "alloc")]
pub mod vec;
#[cfg(feature = "std")]
pub mod vec_hash_set;

pub use arrayref::{array_mut_ref, array_ref};
#[cfg(feature = "std")]
pub use async_trait::*;
#[cfg(feature = "std")]
pub use failure::Fail;
#[cfg(feature = "std")]
pub use lazy_static::*;

pub mod errors {
    pub use base_error::*;
}

#[cfg(feature = "std")]
mod eventually;
#[cfg(feature = "std")]
pub use eventually::*;

pub use base_util::*;
