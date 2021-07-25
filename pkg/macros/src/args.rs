use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Fields, GenericParam, Generics, Index, AttributeArgs
};



pub fn derive_args(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let out = match &input.data {
        Data::Struct(s) => {
            let mut field_names = vec![];
            let mut field_vars = vec![];

            for field in s.fields.iter() {
                let field_name = field.ident.clone().unwrap();
                field_names.push(field_name.clone());

                let field_type = &field.ty;

                let field_name_str = field_name.to_string();

                field_vars.push(quote! {
                    let #field_name = {
                        let value = raw_args.take_named_arg(#field_name_str)?;
                        <#field_type as ::common::args::ArgType>::parse_optional_raw_arg(value)?
                    };
                });

                /*
                let default_attr = field
                    .attrs
                    .iter()
                    .find(|attr| attr.path.is_ident("default".into()));

                let ty = &field.ty;
                let value = if let Some(attr) = default_attr {
                    attr.tokens.clone()
                } else {
                    quote! { <#ty as ::std::default::Default>::default() }
                };
                */

            }

            quote! {
                impl ::common::args::ArgsType for #name {
                    fn parse_raw_args(raw_args: &mut ::common::args::RawArgs) -> ::common::errors::Result<#name> {
                        #(#field_vars)*
                        
                        Ok(#name {
                            #(#field_names,)*
                        })
                    }
                }
            }
        }
        Data::Enum(e) => {
            let mut commands = vec![];

            for var in &e.variants {
                
                let var_name = &var.ident;
                let mut command_name = syn::Lit::Str(syn::LitStr::new(&var.ident.to_string(), var_name.span())) ;

                let arg_attr =
                    var.attrs.iter().find(|attr| attr.path.is_ident("arg"));
                if let Some(attr) = arg_attr {

                    let meta = attr.parse_meta().unwrap();

                    let meta_list = match meta {
                        syn::Meta::List(list) => list.nested,
                        _ => panic!("Unexpected arg attr format")
                    };

                    let mut meta_map = HashMap::new();

                    for meta_item in meta_list {
                        let name_value = match meta_item {
                            syn::NestedMeta::Meta(syn::Meta::NameValue(v)) => v,
                            _ => panic!()
                        };

                        meta_map.insert(name_value.path, name_value.lit);
                    }

                    for (key, value) in meta_map {
                        if key.is_ident("name") {
                            command_name = value;
                        }
                    }
                }

                let fields = match &var.fields {
                    syn::Fields::Unnamed(f) => &f.unnamed,
                    syn::Fields::Unit => {
                        commands.push(quote! {
                            #command_name => { #name::#var_name }
                        });
                        continue;
                    }
                    _ => panic!("Only unnamed enum fields are supported")
                };


                if fields.len() != 1 {
                    panic!("Only one unnamed enum field is supported");
                }

                let field = &fields[0];
                
                let field_type = &field.ty;

                commands.push(quote! {
                    #command_name => {
                        #name::#var_name(<#field_type as ::common::args::ArgsType>::parse_raw_args(raw_args)?)
                    }
                });
            }

            quote! {
                impl ::common::args::ArgsType for #name {
                    fn parse_raw_args(raw_args: &mut ::common::args::RawArgs) -> ::common::errors::Result<#name> {
                        let command_name = raw_args.next_positional_arg()?;
                        Ok(match command_name.as_str() {
                            #(#commands)*
                            _ => {
                                return Err(::common::errors::err_msg("Unknown command"));
                            }
                        })
                    }
                }
            }
        }
        _ => {
            panic!("Unsupported DeriveInput")
        }
    };

    proc_macro::TokenStream::from(out)
}