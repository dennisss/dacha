#[macro_use]
extern crate regexp_macros;

use std::path::PathBuf;


fn main() {
    // tODO: Rebuilt if wrapper.h chanes.

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let lib = pkg_config::probe_library("libsystemd").unwrap();
    let include_paths = lib
        .include_paths
        .iter()
        .map(|p| p.to_str().unwrap())
        .collect::<Vec<_>>();

    let bindings = bindgen::Builder::default()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .header("src/wrapper.h")
        .clang_args(include_paths.iter().map(|path| format!("-I{}", path)))
        .derive_debug(true)
        .derive_default(true)
        .newtype_enum(".*")
        // .blocklist_function(".*")
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

}
