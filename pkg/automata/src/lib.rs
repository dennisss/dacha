extern crate common;
extern crate parsing;

pub mod fsm;
pub mod regexp;

/*
#[macro_export]
macro_rules! regexp {
    ($name:ident => $value:expr) => {
        ::common::lazy_static! {
            static ref $name: ::automata::regexp::RegExp =
                { $crate::regexp::RegExp::new($value).unwrap() };
        }
    };
}
*/