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
use syn::{Item, ItemImpl};

use syn::{Expr, Ident, LitStr, Token};

/*
range_param!(i = 0..100, {
    impl X for [u8; i] {}
})
*/

struct RangeParamInput {
    var: Ident,
    start: usize,
    end: usize,
    code: Expr,
}

impl Parse for RangeParamInput {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        let var: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        let start = input.parse::<LitInt>()?.base10_parse()?;
        input.parse::<Token![..]>()?;
        let end = input.parse::<LitInt>()?.base10_parse()?;

        input.parse::<Token![,]>()?;

        let code = input.parse()?;

        Ok(Self {
            var,
            start,
            end,
            code,
        })
    }
}

// #[proc_macro]
pub fn range_param(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RangeParamInput);

    /*
    let name = def.name;

    let regexp = match automata::regexp::vm::instance::RegExp::new(&def.pattern.value()) {
        Ok(v) => v,
        Err(e) => {
            def.pattern
                .span()
                .unwrap()
                .error(format!("Invalid regular expression: {}", e.to_string()))
                .emit();

            return TokenStream::new();
        }
    };

    let value_tokens = regexp.to_static_codegen().parse::<TokenStream>().unwrap();

    let value = parse_macro_input!(value_tokens as Expr);
    */

    let code = input.code;

    TokenStream::from(quote! {
        fn hello() -> usize {
            12
        }
    })
}
