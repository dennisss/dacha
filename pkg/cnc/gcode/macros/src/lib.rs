#![feature(proc_macro_diagnostic)]

extern crate gcode_decimal;
extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Ident, LitStr, Token};

#[proc_macro]
pub fn command_word(input: TokenStream) -> TokenStream {
    let s = parse_macro_input!(input as LitStr).value();
    let (ks, nums) = s.split_at(1);

    let key = ks.chars().next().unwrap();
    let number = gcode_decimal::Decimal::parse_complete(nums.as_bytes()).unwrap();

    let number_raw = number.to_raw();

    TokenStream::from(quote! {
        CommandWord {
            group: #key,
            number: Decimal::from_raw(#number_raw)
        }
    })
}
