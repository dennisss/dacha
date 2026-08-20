// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/lm_solver.cc");
    println!("cargo:rerun-if-changed=src/lm_solver.h");

    // 1. The 'cc' crate compiles the C++ into machine code
    cc::Build::new()
        .cpp(true)
        .include("/usr/include/eigen3") 
        .file("src/lm_solver.cc")
        .opt_level(3)
        .flag("-ffast-math")
        // .flag("-march=native")
        .compile("lm_solver"); // This outputs liblm_solver.a

    // 2. The 'bindgen' crate reads the header and writes the Rust FFI bindings
    let bindings = bindgen::Builder::default()
        .header("src/lm_solver.h")
        // Tell cargo to invalidate the built crate whenever any of the included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}