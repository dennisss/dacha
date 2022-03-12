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
pub struct Race2<T, A: Future<Output = T>, B: Future<Output = T>> {
    a: A,
    b: B,
}

impl<T, A: Future<Output = T>, B: Future<Output = T>> Future for Race2<T, A, B> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        let this = unsafe { self.get_unchecked_mut() };
        let a = unsafe { Pin::new_unchecked(&mut this.a) };
        let b = unsafe { Pin::new_unchecked(&mut this.b) };

        if let Poll::Ready(v) = a.poll(_cx) {
            return Poll::Ready(v);
        }

        if let Poll::Ready(v) = b.poll(_cx) {
            return Poll::Ready(v);
        }

        return Poll::Pending;
    }
}

pub fn race2<T, A: Future<Output = T>, B: Future<Output = T>>(a: A, b: B) -> Race2<T, A, B> {
    Race2 { a, b }
}
*/

struct RaceInput {
    futures: Vec<Expr>,
}

impl Parse for RaceInput {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        let mut futures = vec![];
        while !input.is_empty() {
            futures.push(input.parse()?);

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self { futures })
    }
}

pub fn race(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RaceInput);

    assert!(input.futures.len() > 1);

    let futures = input.futures;
    let mut types = vec![];
    let mut fields = vec![];
    for (idx, _) in futures.iter().enumerate() {
        types.push(format_ident!("F{}", idx));
        fields.push(format_ident!("f{}", idx));
    }

    TokenStream::from(quote! {
        {
            use ::core::future::Future;
            use ::core::task::{Poll, Context};
            use ::core::pin::Pin;

            struct RaceFuture<T, #(#types: Future<Output = T>,)*> {
                #(#fields: #types,)*
            }

            impl<T, #(#types: Future<Output = T>,)*> Future for RaceFuture<T, #(#types,)*> {
                type Output = T;

                fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
                    let this = unsafe { self.get_unchecked_mut() };

                    #({
                        let p = unsafe { Pin::new_unchecked(&mut this.#fields) };
                        if let Poll::Ready(v) = p.poll(_cx) {
                            return Poll::Ready(v);
                        }
                    })*

                    return Poll::Pending;
                }
            }

            RaceFuture { #(#fields: #futures,)* }
        }
    })
}
