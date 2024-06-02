#![feature(proc_macro_diagnostic)]

extern crate automata;
extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Ident, LitStr, Token};

struct RegExpDeclaraction {
    name: Ident,
    pattern: LitStr,
    flags: Option<LitStr>,
}

impl Parse for RegExpDeclaraction {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        input.parse::<Token![>]>()?;
        let pattern: LitStr = input.parse()?;

        // Second argument is a flags string.
        let mut flags = None;
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
            flags = Some(input.parse()?);
        }

        Ok(Self {
            name,
            pattern,
            flags,
        })
    }
}

/// Statically compiles a given regular expression that can be used at runtime
/// without further compilation.
///
/// Usage:
///     regexp!(NAME => "a(b|c)d");
///     ...
///     assert_eq!(NAME.test("abd"), true);
///
/// TODO: If the expression contains named groups, auto-generate methods in the
/// RegExpMatch object to access them.
///
/// TODO: Based on the character encoding of the expression, we should be able
/// to safely cast some groups to an &str.
#[proc_macro]
pub fn regexp(input: TokenStream) -> TokenStream {
    let def = parse_macro_input!(input as RegExpDeclaraction);

    let name = def.name;

    let flags = def.flags.map(|v| v.value()).unwrap_or_default();

    let regexp = match automata::regexp::vm::instance::RegExp::new_with_flags(
        &def.pattern.value(),
        &flags,
    ) {
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

    TokenStream::from(quote! {
        static #name: ::automata::regexp::vm::instance::StaticRegExp = #value;
    })
}
