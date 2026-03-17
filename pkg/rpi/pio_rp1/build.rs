extern crate bindgen;

use std::env;
use std::path::PathBuf;

use bindgen::callbacks::{ParseCallbacks, Token};


// This rewrites "_u(1) into "1" so that bindgen can parse the constants. 
#[derive(Debug)]
struct Callbacks;

impl ParseCallbacks for Callbacks {
    fn modify_macro(&self, _name: &str, tokens: &mut Vec<Token>) {
        // Ensure we have at least the 5 tokens we expect: NAME, _u, (, VALUE..., )
        if tokens.len() >= 5 {
            // Use .as_ref() to borrow the Box<[u8]> as a slice &[u8]
            // and use &b"..."[..] to ensure the right side is also a slice &[u8]
            let is_wrapper = tokens[1].raw.as_ref() == &b"_u"[..] 
                || tokens[1].raw.as_ref() == &b"_ul"[..] 
                || tokens[1].raw.as_ref() == &b"_ull"[..];

            let is_open_paren = tokens[2].raw.as_ref() == &b"("[..];
            
            let is_close_paren = tokens.last()
                .map(|t| t.raw.as_ref() == &b")"[..])
                .unwrap_or(false);

            if is_wrapper && is_open_paren && is_close_paren {
                // 1. Remove the closing `)` at the very end 
                tokens.pop();
                
                // 2. Remove the opening `(` at index 2
                tokens.remove(2);
                
                // 3. Remove the `_u` at index 1
                tokens.remove(1);
            }
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .rustified_enum(".*")
        .allowlist_type(".*")
        .allowlist_var(".*")
        .blocklist_function(".*")
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .parse_callbacks(Box::new(Callbacks {}))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
