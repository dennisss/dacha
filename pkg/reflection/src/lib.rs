extern crate proc_macro;
extern crate syn;

#[macro_use] extern crate quote;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, parse_quote, Data, DeriveInput, Fields,
		  GenericParam, Generics, Index};


struct ReflectField<'a> {
	tags: &'static [&'static str],
	value: ReflectValue<'a>
}

enum ReflectValue<'a> {
	String(&'a mut String),
	U64(&'a mut u64)
}


//pub trait Reflect {
//
////	fn reflect(&mut self)
//}

macro_rules! ast_string {
    ($val:expr) => {{
    	let v = $val;
    	let s = quote! { #v };
    	s.to_string()
    }};
}


#[proc_macro_derive(Reflect, attributes(tags))]
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
			for field in s.fields.iter() {
				let field_name = field.ident.clone().unwrap();

				let typename = ast_string!(&field.ty);

				let var = match typename.as_str() {
					"String" => {
						typename
					},
					"u64" => "U64".to_string(),
					_ => panic!("Unknown type")
				};
				let var = format_ident!("{}", var);

//				for attr in &field.attrs {
//					let name = ast_string!(&attr.path);
//					let value = ast_string!(&attr.tokens);
//
//					println!("{} => {}", name, value);
//				}

//				println!("{:#?}", field.attrs);

				let val = quote! {
					ReflectField {
						tags: &[],
						value: ReflectValue::#var(&mut self.#field_name)
					}
				};

//				arms.push(quote! {
//					#field_name => Some(#val),
//				});

				fields.push(val);
			}
		},
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
//        impl #impl_generics heapsize::HeapSize for #name #ty_generics #where_clause {
//            fn heap_size_of_children(&self) -> usize {
//                #sum
//            }
//        }
//    };

	let out = quote! {
		impl #name {
			pub fn fields_mut<'a, F: FnMut(ReflectField<'a>)>(
				&'a mut self, mut f: F) {
				#(f(#fields);)*
			}
		}
	};

	// We'd need to lookup

	// Hand the output tokens back to the compiler.
	proc_macro::TokenStream::from(out)
}



#[proc_macro_derive(Defaultable, attributes(default))]
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
}