use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, AttributeArgs, Data, DeriveInput, Fields, GenericParam,
    Generics, Index, Lit, Path, Type,
};

use crate::utils::get_options;

const ATTR_NAME: &'static str = "arg";

pub fn derive_args(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let out = match &input.data {
        Data::Struct(s) => {
            let fields_parser = derive_fields_parser(quote! { #name }, &s.fields);

            quote! {
                impl ::common::args::ArgsType for #name {
                    fn parse_raw_args(raw_args: &mut ::common::args::RawArgs) -> ::common::errors::Result<#name> {
                        #fields_parser
                    }
                }

                impl ::common::args::ArgFieldType for #name {
                    fn parse_raw_arg_field(field_name: &str, raw_args: &mut ::common::args::RawArgs) -> Result<#name> {
                        // NOTE: The field_name is ignored.
                        <#name as ::common::args::ArgsType>::parse_raw_args(raw_args)
                    }
                }
            }
        }
        Data::Enum(e) => {
            let mut commands = vec![];

            let mut all_unit_variants = true;

            for var in &e.variants {
                let var_name = &var.ident;
                let mut command_name =
                    syn::Lit::Str(syn::LitStr::new(&var.ident.to_string(), var_name.span()));

                let options = get_options(ATTR_NAME, &var.attrs);
                for (key, value) in options {
                    if key.is_ident("name") {
                        command_name = value.unwrap();
                    }
                }

                let fields = match &var.fields {
                    // e.g. 'Enum::Variant(T)'
                    syn::Fields::Unnamed(f) => {
                        all_unit_variants = false;

                        let fields = &f.unnamed;

                        if fields.len() != 1 {
                            panic!("Only one unnamed enum field is supported");
                        }

                        let field = &fields[0];

                        let field_type = &field.ty;

                        commands.push(quote! {
                            #command_name => {
                                Ok(#name::#var_name(<#field_type as ::common::args::ArgsType>::parse_raw_args(raw_args)?))
                            }
                        });
                    }
                    // e.g. 'Enum::Variant { key: String }'
                    syn::Fields::Named(_) => {
                        all_unit_variants = false;

                        let fields_parser =
                            derive_fields_parser(quote! { #name::#var_name }, &var.fields);

                        commands.push(quote! {
                            #command_name => { #fields_parser }
                        });
                    }
                    // e.g. 'Enum::Variant' (with no data)
                    syn::Fields::Unit => {
                        commands.push(quote! {
                            #command_name => { Ok(#name::#var_name) }
                        });
                    }
                    _ => panic!("Only unnamed or unit enum fields are supported"),
                };
            }

            // When all of the variants have no data, we can always represent it as just a
            // single argument.
            if all_unit_variants {
                let out = quote! {
                    impl ::common::args::ArgType for #name {
                        fn parse_raw_arg(raw_arg: ::common::args::RawArgValue) -> ::common::errors::Result<Self> {
                            let command_name = match raw_arg {
                                ::common::args::RawArgValue::String(s) => s,
                                _ => { return Err(::common::errors::err_msg("Expected string argument")); },
                            };

                            match command_name.as_str() {
                                #(#commands)*
                                _ => {
                                    Err(::common::errors::err_msg("Unknown command"))
                                }
                            }
                        }
                    }
                };

                return proc_macro::TokenStream::from(out);
            }

            quote! {
                impl ::common::args::ArgsType for #name {
                    fn parse_raw_args(raw_args: &mut ::common::args::RawArgs) -> ::common::errors::Result<#name> {
                        let command_name = raw_args.next_positional_arg()
                            .ok_or_else(|| ::common::errors::err_msg("Expected another positional argument"))?;
                        match command_name.as_str() {
                            #(#commands)*
                            _ => {
                                Err(::common::errors::err_msg("Unknown command"))
                            }
                        }
                    }
                }

                impl ::common::args::ArgFieldType for #name {
                    fn parse_raw_arg_field(field_name: &str, raw_args: &mut ::common::args::RawArgs) -> Result<#name> {
                        // NOTE: The field_name is ignored.
                        <#name as ::common::args::ArgsType>::parse_raw_args(raw_args)
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

fn derive_fields_parser(
    struct_name: proc_macro2::TokenStream,
    fields: &Fields,
) -> proc_macro2::TokenStream {
    let mut field_names = vec![];
    let mut field_vars = vec![];

    for field in fields.iter() {
        let field_name = field.ident.clone().unwrap();
        field_names.push(field_name.clone());

        let field_type = &field.ty;

        let field_name_str = field_name.to_string();

        let options = get_options(ATTR_NAME, &field.attrs);

        let mut positional = false;
        let mut default_value = None;
        for (key, value) in options {
            if key.is_ident("default") {
                let v = value.unwrap();
                default_value = Some(quote! { #v });
            } else if key.is_ident("positional") {
                positional = true;
            }

            // TODO: Support 'name' and 'short'
        }

        if let Type::Path(path) = field_type {
            if path.path.is_ident("String") {
                default_value = default_value.map(|v| {
                    quote! {
                        #v.to_string()
                    }
                });
            }
        }

        if positional {
            // NOTE: Positional arguments can only be None if hit the end of the arguments
            // list (and all following positional arguments also are None).
            field_vars.push(quote! {
                let #field_name = {
                    let value = raw_args.next_positional_arg()
                        .map(|s| ::common::args::RawArgValue::String(s));
                    <#field_type as ::common::args::ArgType>::parse_optional_raw_arg(value)?
                };
            });
        } else if let Some(default_value) = default_value {
            field_vars.push(quote! {
                let #field_name = {
                    let value = raw_args.take_named_arg(#field_name_str)?;
                    if let Some(v) = value {
                        <#field_type as ::common::args::ArgType>::parse_raw_arg(v)?
                    } else {
                        #default_value
                    }
                };
            });
        } else {
            field_vars.push(quote! {
                let #field_name = {
                    <#field_type as ::common::args::ArgFieldType>::parse_raw_arg_field(#field_name_str, raw_args)?
                };
            });
        }
    }

    quote! {
        #(#field_vars)*

        Ok(#struct_name {
            #(#field_names,)*
        })
    }
}
