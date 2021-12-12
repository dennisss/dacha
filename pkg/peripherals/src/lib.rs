#![no_std]

// #[cfg(feature = "std")]
// extern crate std;

pub mod nrf52840 {
    #![allow(dead_code, non_snake_case, non_camel_case_types)]

    use super::enum_def_with_unknown;

    include!(concat!(env!("OUT_DIR"), "/nrf52840.rs"));
}

pub use nrf52840::*;

// TODO: Deduplicate this
#[macro_export]
macro_rules! enum_def_with_unknown {
    // TODO: Derive a smarter hash
    ($(#[$meta:meta])* $name:ident $t:ty => $( $case:ident = $val:expr ),*) => {
    	$(#[$meta])*
        #[derive(Clone, Copy, Debug)]
		pub enum $name {
			$(
				$case,
			)*
            Unknown($t)
		}

		impl $name {
			pub fn from_value(v: $t) -> Self {
				match v {
					$(
						$val => $name::$case,
					)*
					_ => {
                        $name::Unknown(v)
					}
				}
			}

			pub fn to_value(&self) -> $t {
				match self {
					$(
						$name::$case => $val,
					)*
                    $name::Unknown(v) => *v
				}
			}
		}

        impl ::core::cmp::PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.to_value() == other.to_value()
            }
        }

        impl ::core::cmp::Eq for $name {}

        impl ::core::hash::Hash for $name {
            fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                self.to_value().hash(state);
            }
        }
    };
}
