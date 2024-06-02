use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Fields, GenericParam, Generics, Ident,
    Index, Lit,
};

use crate::utils::get_options;

const PARSE_ATTR_NAME: &'static str = "parse";

macro_rules! ast_string {
    ($val:expr) => {{
        let v = $val;
        let s = quote! { #v };
        s.to_string()
    }};
}

pub fn derive_reflection(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    //	println!("{:#?}", input.data);

    //	let mut arms = vec![];

    let mut fields = vec![];

    match &input.data {
        Data::Struct(s) => {
            for (i, field) in s.fields.iter().enumerate() {
                let field_name = field.ident.clone().unwrap();

                let typename = ast_string!(&field.ty);

                let var = match typename.as_str() {
                    "String" => typename,
                    "u64" => "U64".to_string(),
                    "i64" => "I64".to_string(),
                    "u32" => "U32".to_string(),
                    "i32" => "I32".to_string(),
                    "u16" => "U16".to_string(),
                    "i16" => "I16".to_string(),
                    "u8" => "U8".to_string(),
                    s @ _ => {
                        if s.starts_with("[u8;") {
                            "U8Slice".to_string()
                        } else {
                            panic!("Unknown type {}", s)
                        }
                    }
                };
                let var = format_ident!("{}", var);

                let mut tags = vec![];

                for attr in &field.attrs {
                    let name = ast_string!(&attr.path);
                    //                    let value = ast_string!(&attr.tokens);

                    if name == "tags" {
                        let mut outer_toks = attr.tokens.clone().into_iter().collect::<Vec<_>>();
                        if outer_toks.len() != 1 {
                            panic!("Expected a token group for tags");
                        }

                        let group = if let TokenTree::Group(g) = outer_toks.pop().unwrap() {
                            g
                        } else {
                            panic!("Expected a token group for tags");
                        };

                        let toks = group.stream().into_iter().collect::<Vec<_>>();

                        let mut i = 0;
                        while i < toks.len() {
                            let name = if let TokenTree::Ident(ident) = &toks[i] {
                                ident.to_string()
                            } else {
                                panic!("Expected ident");
                            };

                            i += 1;

                            {
                                let punc = if let TokenTree::Punct(punc) = &toks[i] {
                                    punc
                                } else {
                                    panic!("Expected punc");
                                };

                                if punc.as_char() != '=' {
                                    panic!("Expected =");
                                }

                                i += 1;
                            }

                            let lit = if let TokenTree::Literal(l) = &toks[i] {
                                l
                            } else {
                                panic!("Expected lit");
                            };

                            i += 1;

                            let t = quote! {
                                ::reflection::ReflectTag { key: #name, value: #lit }
                            };
                            // println!("{}", ast_string!(&t));
                            tags.push(t);

                            if i < toks.len() {
                                let punc = if let TokenTree::Punct(punc) = &toks[i] {
                                    punc
                                } else {
                                    panic!("Expected punc");
                                };

                                if punc.as_char() != ',' {
                                    panic!("Expected ,");
                                }

                                i += 1;
                            }
                        }
                    }
                }

                let val = quote! {
                    #i => ::reflection::ReflectField {
                        tags: &[ #(#tags,)* ],
                        value: ::reflection::ReflectValue::#var(
                            &mut self.#field_name)
                    }
                };

                fields.push(val);
            }
        }
        _ => {}
    }

    // Add a bound `T: HeapSize` to every type parameter T.
    //	let generics = add_trait_bounds(input.generics);
    //	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    //
    //	// Generate an expression to sum up the heap size of each field.
    //	let sum = heap_size_sum(&input.data);
    //
    //	let expanded = quote! {
    //        // The generated impl.
    //        impl #impl_generics heapsize::HeapSize for #name #ty_generics
    // #where_clause {            fn heap_size_of_children(&self) -> usize {
    //                #sum
    //            }
    //        }
    //    };

    //	let iter_mut_name = format_ident!("{}FieldIterMut", name);

    let num_fields = fields.len();

    let out = quote! {
        impl ::reflection::Reflect for #name {
            fn fields_index_mut<'a>(&'a mut self, index: usize)
                -> ::reflection::ReflectField<'a> {
                match index {
                    #(#fields,)*
                    _ => panic!("Field index out of range")
                }
            }

            fn fields_len(&self) -> usize { #num_fields }
        }
    };

    // We'd need to lookup

    // Hand the output tokens back to the compiler.
    proc_macro::TokenStream::from(out)
}

pub fn derive_parseable(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut parse_early = vec![];
    let mut parse_values = vec![];
    let mut parse_branch = vec![];
    let mut parse_fields = vec![];
    let mut serialize_fields = vec![];

    let struct_options = get_options(PARSE_ATTR_NAME, &input.attrs);

    let mut allow_unknown = false;
    for (key, value) in struct_options {
        if key.is_ident("allow_unknown") {
            let v = value.unwrap();
            match &v {
                Lit::Bool(v) => {
                    allow_unknown = v.value;
                }
                _ => panic!("Bad allow_unknown format"),
            };
        }
    }

    match &input.data {
        Data::Struct(s) => {
            for (i, field) in s.fields.iter().enumerate() {
                // let typename = ast_string!(&field.ty);

                let field_ty = &field.ty;
                let field_ident = field.ident.clone().unwrap();
                let mut field_name = field_ident.to_string();

                let mut is_sparse = false;
                let mut flatten = false;

                let options = get_options(PARSE_ATTR_NAME, &field.attrs);
                for (key, value) in options {
                    if key.is_ident("name") {
                        let v = value.unwrap();
                        field_name = match &v {
                            Lit::Str(s) => s.value().to_string(),
                            _ => panic!("Bad name format"),
                        };

                        continue;
                    }

                    if key.is_ident("sparse") {
                        let v = value.unwrap();
                        is_sparse = match &v {
                            Lit::Bool(v) => v.value(),
                            _ => panic!("Bad sparse format"),
                        };

                        continue;
                    }

                    if key.is_ident("flatten") {
                        let v = value.unwrap();
                        flatten = match &v {
                            Lit::Bool(v) => v.value(),
                            _ => panic!("Bad flatten format"),
                        };

                        continue;
                    }
                }

                let field_value = format_ident!("{}_value", field_ident);

                if flatten {
                    parse_values.push(quote! {
                        #field_value: #field_ty::Builder
                    });

                    parse_early.push(quote! {
                        let (key, value) = match self.#field_value.add_field(key, value)? {
                            Some(v) => v,
                            None => return Ok(None)
                        };
                    });

                    parse_fields.push(quote! {
                        #field_ident: self.#field_value.build()?
                    });

                    // TODO: Add to serialization.

                    continue;

                }

                parse_values.push(quote! {
                    #field_value: Option<#field_ty>
                });

                parse_branch.push(quote! {
                    #field_name => {
                        if let Some(existing_value) = &mut self.#field_value {
                            <#field_ty as ::reflection::ParseFromValue<'data>>::parse_merge(existing_value, value)
                                .map_err(|e| format_err!("[{}] {}", #field_name, e))?;
                        } else {
                            self.#field_value = Some(
                                <#field_ty as ::reflection::ParseFrom<'data>>::parse_from(value)
                                    .map_err(|e| format_err!("[{}] {}", #field_name, e))?
                            );
                        }                        
                    }
                });

                if is_sparse {
                    parse_fields.push(quote! {
                        #field_ident: self.#field_value.unwrap_or_else(|| <#field_ty as Default>::default())
                    });

                    serialize_fields.push(quote! {
                        if !<#field_ty as ::reflection::SerializeTo>::serialize_sparse_as_empty_value(&self.#field_ident) {
                            ::reflection::ObjectSerializer::serialize_field(&mut obj, #field_name, &self.#field_ident)?;
                        }
                    });

                    continue;
                }

                // TODO: Inject the absolute field path into this function.
                parse_fields.push(quote! {
                    #field_ident: <#field_ty as ::reflection::ParseFromValue<'data>>::unwrap_parsed_result(
                        #field_name, self.#field_value)?
                });

                serialize_fields.push(quote! {
                    ::reflection::ObjectSerializer::serialize_field(&mut obj, #field_name, &self.#field_ident)?;
                });
            }
        }
        _ => {}
    }

    let name_string = name.to_string();

    let name_builder = format_ident!("{}Builder", name);

    let visibility = input.vis;

    // TODO: Is the order of execution defined for which fields will be parsed
    // first.
    let out = quote! {

        #[derive(Default)]
        #visibility struct #name_builder {
            #(#parse_values,)*
        }

        impl<'data> ::reflection::ObjectBuilder<'data> for #name_builder {
            type ObjectType = #name;

            fn add_field<V: ::reflection::ValueReader<'data>>(
                &mut self,
                key: String,
                value: V,
            ) -> Result<Option<(String, V)>> {
                use ::reflection::ObjectBuilder;

                #(#parse_early)*

                let key_s: &str = key.as_ref();
                match key_s {
                    #(#parse_branch,)*
                    _ => {
                        return Ok(Some((key, value)));
                    }
                }

                Ok(None)
            }

            fn build(self) -> Result<Self::ObjectType> {
                use ::reflection::ObjectBuilder;

                Ok(#name {
                    #(#parse_fields,)*
                })
            }

        }

        impl #name {
            pub type Builder = #name_builder;

            pub fn builder() -> #name_builder {
                #name_builder::default()
            }
        }


        impl<'data> ::reflection::ParseFromValue<'data> for #name {
            fn parse_from_object<Input: ::reflection::ObjectIterator<'data>>(mut input: Input) -> Result<Self> {
                use ::reflection::ObjectBuilder;

                let mut builder = Self::builder();

                while let Some((key, value)) = ::reflection::ObjectIterator::next_field(&mut input)? {
                    if let Some((key, _)) = builder.add_field(key, value)? {
                        if !#allow_unknown {
                            return Err(format_err!("Unknown field: {}", key));
                        }
                    }
                }

                builder.build()
            }

            fn parsing_hint() -> Option<::reflection::ParsingTypeHint> {
                Some(::reflection::ParsingTypeHint::Object)
            }

            fn parsing_typename() -> Option<&'static str> {
                Some(#name_string)
            }
        }

        impl ::reflection::SerializeTo for #name {
            fn serialize_to<Output: ::reflection::ValueSerializer>(&self, output: Output) -> Result<()> {
                let mut obj = output.serialize_object();
                #(#serialize_fields)*
                Ok(())
            }
        }
    };

    proc_macro::TokenStream::from(out)
}

pub fn derive_defaultable(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut fields = vec![];

    match &input.data {
        Data::Struct(s) => {
            for field in s.fields.iter() {
                let field_name = field.ident.clone().unwrap();

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

                fields.push(quote! {
                    #field_name: #value
                });
            }
        }
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
}

pub fn derive_const_default(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut fields = vec![];

    match &input.data {
        Data::Struct(s) => {
            for field in s.fields.iter() {
                let field_name = field.ident.clone().unwrap();

                let ty = &field.ty;
                let value = {
                    quote! { <#ty as ::common::const_default::ConstDefault>::DEFAULT }
                };

                fields.push(quote! {
                    #field_name: #value
                });
            }
        }
        _ => {}
    }

    let out = quote! {
        impl ::common::const_default::ConstDefault for #name {
            const DEFAULT: Self = #name {
                #(#fields,)*
            };
        }
    };

    proc_macro::TokenStream::from(out)
}

pub fn derive_errable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let out = quote! {
        // impl ::common::errors::error_new::Errable for #name {
        //     fn as_any<'a>(&'a self) -> &'a dyn ::core::any::Any {
        //         self
        //     }
        // }

        impl ::common::errors::error_new::ErrableCode for #name {
            fn from_error_code(code: u32) -> Self {
                // We assume that the enum has repr(u32)
                // TODO: Eventually validate the above statement.
                unsafe { ::core::mem::transmute(code) }
            }

            fn error_code(&self) -> u32 {
                *self as u32
            }
        }
    };

    proc_macro::TokenStream::from(out)
}
