extern crate proc_macro;
extern crate proc_macro2;
extern crate syn;
extern crate uuid;
#[macro_use]
extern crate quote;

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

#[proc_macro]
pub fn uuid(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr).value();

    let id = uuid::UUID::parse(&input).unwrap();
    let data: &[u8] = id.as_ref();

    TokenStream::from(quote! {
        ::uuid::UUID::new([#( #data, )*])
    })
}
