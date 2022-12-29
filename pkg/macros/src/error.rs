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
use syn::{Item, ItemFn, ItemImpl};

pub fn error(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let name = input.ident.clone();

    let code = quote! {
        #[derive(Debug)]
        #input

        impl ::core::fmt::Display for #name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::result::Result<(), ::core::fmt::Error> {
                ::core::fmt::Debug::fmt(self, f)
            }
        }

        impl ::std::error::Error for #name {}

    };

    TokenStream::from(code)
}
