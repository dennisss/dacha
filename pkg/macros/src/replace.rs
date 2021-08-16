// Based on the code at the bottom of https://stackoverflow.com/a/55953279

use proc_macro::{token_stream, Ident, TokenStream, TokenTree};

#[proc_macro]
pub fn replace(input: TokenStream) -> TokenStream {
    let mut it = input.into_iter();

    // Get first parameters
    let needle = get_ident(&mut it);
    let _comma = it.next().unwrap();
    let replacement = get_ident(&mut it);
    let _comma = it.next().unwrap();

    replace_inner(it, &needle, &replacement)
}

fn replace_inner(it: token_stream::IntoIter, needle: &Ident, replacement: &Ident) -> TokenStream {
    // Return the remaining tokens, but replace identifiers.
    it.map(|tt| {
        match tt {
            // Comparing `Ident`s can only be done via string comparison right
            // now. Note that this ignores syntax contexts which can be a
            // problem in some situation.
            TokenTree::Ident(ref i) if i.to_string() == needle.to_string() => {
                TokenTree::Ident(replacement.clone())
            }
            TokenTree::Group(g) => {
                let delimiter = g.delimiter();
                let stream = replace_inner(g.stream().into_iter(), &needle, &replacement);
                let mut new_g = proc_macro::Group::new(delimiter, stream);
                new_g.set_span(g.span());

                TokenTree::Group(new_g)
            }
            // All other tokens are just forwarded
            other => other,
        }
    })
    .collect()
}

/// Extract an identifier from the iterator.
fn get_ident(it: &mut token_stream::IntoIter) -> Ident {
    match it.next() {
        Some(TokenTree::Ident(i)) => i,
        Some(TokenTree::Group(g)) => {
            println!("{:?}", g);
            get_ident(&mut g.stream().into_iter())
            // TODO: Check that nothing is after the group
        }
        t @ _ => panic!("oh noes! {:?}", t),
    }
}
