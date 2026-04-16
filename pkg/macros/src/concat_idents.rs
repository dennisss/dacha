// Compatible re-implementation of the old concat_idents unstable Rust macro since it is removed in new Rust versions.

use proc_macro::{Ident, Span, TokenStream, TokenTree};

pub fn concat_idents(input: TokenStream) -> TokenStream {
    let mut concatenated_name = String::new();

    // Iterate through the tokens passed into the macro
    for token in input {
        match token {
            // If it's an identifier, append its string representation
            TokenTree::Ident(ident) => {
                concatenated_name.push_str(&ident.to_string());
            }
            // If it's a comma, just ignore it and move on
            TokenTree::Punct(punct) if punct.as_char() == ',' => {
                continue;
            }
            // For anything else (numbers, brackets, etc.), throw a compile error
            _ => panic!("concat_idents! only accepts comma-separated identifiers"),
        }
    }

    if concatenated_name.is_empty() {
        panic!("concat_idents! requires at least one identifier");
    }

    // Create a brand new identifier token from our concatenated string.
    // Span::call_site() tells the compiler to treat this new identifier 
    // as if it was written exactly where the macro was called.
    let new_ident = Ident::new(&concatenated_name, Span::call_site());

    // Wrap the single token back into a TokenStream and return it
    TokenStream::from(TokenTree::Ident(new_ident))
}