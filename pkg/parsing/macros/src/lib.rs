#[macro_use]
extern crate quote;

use std::collections::HashMap;

use parsing::Endian;
use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, AttributeArgs, Block, Data, DeriveInput, Expr, Fields,
    GenericParam, Generics, Ident, Index, Item, ItemFn, ItemImpl, Lit, LitInt, LitStr, Meta,
    NestedMeta, Path, Result, Token,
};

/*
I need to go through all the fields and parse each of them.

Though some need to be

#[bin(bit_width = 12)]

*/

struct Args {
    endian: parsing::Endian,
}

fn lit_to_string(lit: Lit) -> String {
    match lit {
        Lit::Str(s) => s.value(),
        _ => todo!(),
    }
}

#[proc_macro_attribute]
pub fn bin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let args = {
        let raw = parse_macro_input!(attr as AttributeArgs);
        let map = get_args(raw);

        // println!("{:?}", map);
        // todo!();

        let mut endian = None;

        for (p, v) in map {
            if p.is_ident("endian") {
                endian = Some(match lit_to_string(v.unwrap()).as_str() {
                    "big" => Endian::Big,
                    "little" => Endian::Little,
                    v @ _ => panic!("Unknown endian: {}", v),
                });
            } else {
                todo!()
            }
        }

        Args {
            endian: endian.unwrap(),
        }
    };

    let name = input.ident.clone();

    let code = match &input.data {
        Data::Struct(s) => {
            let mut field_parsers = vec![];

            let mut field_assigners = vec![];

            let mut struct_size = quote! { Some(0) };

            assert!(
                !s.fields.is_empty(),
                "Binary struct must have at least one field"
            );

            let mut is_tuple_struct = s.fields.iter().next().unwrap().ident.is_none();
            for (i, f) in s.fields.iter().enumerate() {
                let name = f.ident.clone().unwrap_or(format_ident!("unnamed{}", i));
                let value_var = format_ident!("{}_value", name);
                let value_ty = f.ty.clone();

                let endian = format_ident!("{}", format!("{:?}", args.endian));

                field_parsers.push(quote! {
                    let #value_var = match <#value_ty as ::parsing::BinaryRepr>::parse_from_bytes(
                        input, ::parsing::Endian::#endian) {
                        Ok((v, rest)) => {
                            input = rest;
                            v
                        }
                        Err(e) => return Err(e)
                    };
                });

                field_assigners.push({
                    if is_tuple_struct {
                        quote! { #value_var }
                    } else {
                        quote! {
                            #name: #value_var
                        }
                    }
                });

                struct_size = quote! {
                    ::parsing::add_size_of(#struct_size, <#value_ty as ::parsing::BinaryRepr>::SIZE_OF)
                };
            }

            let ctor = {
                if is_tuple_struct {
                    quote! { Self(#(#field_assigners)*) }
                } else {
                    quote! { Self { #(#field_assigners),* } }
                }
            };

            quote! {
                impl ::parsing::BinaryRepr for #name {
                    const SIZE_OF: Option<usize> = #struct_size;

                    fn parse_from_bytes<'a>(mut input: &'a [u8], endian: ::parsing::Endian)
                        -> ::common::errors::Result<(Self, &'a [u8])> {

                        #(#field_parsers)*

                        Ok((#ctor, input))
                    }
                }
            }
        }
        Data::Enum(_) => todo!(),
        Data::Union(_) => todo!(),
    };

    // input.data

    let code = quote! {
        #[derive(Debug)]
        #input

        #code
    };

    TokenStream::from(code)
}

fn get_args(raw: AttributeArgs) -> HashMap<Path, Option<Lit>> {
    let mut meta_map = HashMap::new();

    for meta in raw {
        match meta {
            NestedMeta::Meta(Meta::NameValue(name_value)) => {
                // TODO: Assert no duplicates.
                meta_map.insert(name_value.path, Some(name_value.lit));
            }
            _ => todo!(),
        }
    }

    meta_map
}

fn get_options(attr_name: &str, attrs: &[syn::Attribute]) -> HashMap<Path, Option<Lit>> {
    let mut meta_map = HashMap::new();

    let arg_attr = attrs.iter().find(|attr| attr.path.is_ident(attr_name));
    if let Some(attr) = arg_attr {
        let meta = attr.parse_meta().unwrap();

        let meta_list = match meta {
            syn::Meta::List(list) => list.nested,
            _ => panic!("Unexpected arg attr format"),
        };

        for meta_item in meta_list {
            let name_value = match meta_item {
                syn::NestedMeta::Meta(syn::Meta::NameValue(v)) => v,
                syn::NestedMeta::Meta(syn::Meta::Path(p)) => {
                    meta_map.insert(p, None);
                    continue;
                }
                _ => panic!("Unsupported meta_item: {:?}", meta_item),
            };

            meta_map.insert(name_value.path, Some(name_value.lit));
        }
    }

    meta_map
}
