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

pub fn testcase(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    assert!(
        input.attrs.is_empty(),
        "#[testcase] doesn't support function attributes."
    );
    assert!(
        input.sig.generics.params.is_empty() && input.sig.generics.where_clause.is_none(),
        "#[testcase] doesn't support generic functions."
    );
    assert!(
        input.sig.inputs.is_empty(),
        "#[testcase] doesn't support function arguments."
    );

    let test_name = input.sig.ident;
    let test_inner = input.block;
    let is_async = input.sig.asyncness.is_some();

    let body = {
        if is_async {
            quote! {
                ::executor::run(async {
                    #test_inner
                })
            }
        } else {
            quote! { #test_inner }
        }
    };

    let func = quote! {
        #[test]
        fn #test_name() {
            #body
        }
    };

    TokenStream::from(func)
}
