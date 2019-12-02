
extern crate proc_macro;
extern crate syn;

#[macro_use] extern crate quote;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, parse_quote, Data, DeriveInput, Fields,
		  GenericParam, Generics, Index};

use syn::Result;
use syn::{Item, ItemImpl};
use syn::parse::{Parse, ParseStream};

#[derive(Debug)]
struct BlanketInput {
	impls: Vec<ItemImpl>,
}

impl Parse for BlanketInput {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            impls: {
                let mut items = Vec::new();
                while !input.is_empty() {
                    items.push(input.parse()?);
                }
                items
            },
        })
    }
}

#[proc_macro]
pub fn blanket(input: TokenStream) -> TokenStream {
	println!("PARSING TREE");

	// Parse the input tokens into a syntax tree.
	let input = parse_macro_input!(input as BlanketInput);

	println!("{:#?}", input);

	// Take the first impl block

	// Record all generics defined in it

	// For each other impl, redefine it using the additional generics
	
	let blocks = input.impls;
	proc_macro::TokenStream::from(quote! { #(#blocks)* })

	/*

	// Used in the quasi-quotation below as `#name`.
	let name = input.ident;

	let mut fields = vec![];

	match &input.data {
		Data::Struct(s) => {
			for field in s.fields.iter() {
				let field_name = field.ident.clone().unwrap();

				let default_attr = field.attrs.iter().find(|attr| {
					attr.path.is_ident("default".into())
				});

				let ty = &field.ty;
				let value =
					if let Some(attr) = default_attr {
						attr.tokens.clone()
					} else {
						quote! { <#ty as ::std::default::Default>::default() }
					};

				fields.push(quote! {
					#field_name: #value
				});
			}
		},
		_ => {}
	}

	let out = quote! {
		impl ::std::default::Default for #name {
			fn default() -> #name {
				#name {
					#(#fields,)*
				}
			}
		}
	};

	proc_macro::TokenStream::from(out)
	*/
}