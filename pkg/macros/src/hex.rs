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

/// Implements the macro hex!(..) which takes a hex string as input and returns
/// the '[u8; _]' which
pub fn hex(input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as LitStr).value();
    input.retain(|c| !c.is_whitespace());

    let data = radix::hex_decode(&input);

    TokenStream::from(quote! {
        [#( #data, )*]
    })
}
