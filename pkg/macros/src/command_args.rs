use std::str::FromStr;

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Fields, GenericParam, Generics, Index,
    LitInt,
};
use syn::{Block, Result};
use syn::{Expr, Ident, LitStr, Token};
use syn::{Item, ItemImpl};

pub fn command_args(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr).value();

    let mut args: Vec<proc_macro2::TokenStream> = vec![];

    let mut last_part = String::new();
    let mut in_escaped = false;
    for c in input.chars() {
        if in_escaped {
            if c == '}' {
                args.push(proc_macro2::TokenStream::from_str(&last_part).unwrap());
                in_escaped = false;
                last_part = String::new();
                continue;
            }

            last_part.push(c);
        } else {
            if c == '{' {
                assert!(last_part.is_empty());
                in_escaped = true;
                continue;
            }

            if c.is_ascii_whitespace() {
                if !last_part.is_empty() {
                    args.push(proc_macro2::TokenStream::from(quote! { #last_part }));
                    last_part = String::new();
                }

                continue;
            }

            last_part.push(c);
        }
    }

    if !last_part.is_empty() {
        args.push(proc_macro2::TokenStream::from(quote! { #last_part }));
        last_part = String::new();
    }

    let name = args[0].clone();
    let rest = args[1..].to_vec();

    TokenStream::from(quote! {
        ::std::process::Command::new(#name)#( .arg(#rest) )*
    })
}