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

pub fn run(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    assert!(
        input.attrs.is_empty(),
        "#[executor_main] doesn't support function attributes."
    );
    assert!(
        input.sig.generics.params.is_empty() && input.sig.generics.where_clause.is_none(),
        "#[executor_main] doesn't support generic functions."
    );
    assert!(
        input.sig.inputs.is_empty(),
        "#[executor_main] doesn't support function arguments."
    );

    let func_name = input.sig.ident;
    let func_inner = input.block;
    let is_async = input.sig.asyncness.is_some();
    let return_type = input.sig.output;

    let body = {
        if is_async {
            quote! {
                ::executor::run_main(async {
                    #func_inner
                }).unwrap()
            }
        } else {
            quote! { #func_inner }
        }
    };

    let func = quote! {
        fn main() #return_type {
            #body
        }
    };

    TokenStream::from(func)
}
