use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Fields, GenericParam, Generics, Index,
};

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
                        if s.starts_with("[u8 ;") {
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
