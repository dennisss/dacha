use std::path::PathBuf;

fn run_bindgen(include_paths: &[&str]) {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Bindgen is only used for generating Rust bindings for trivial copyable
    // structs.
    let bindings = bindgen::Builder::default()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .header("src/bindings.h")
        .clang_args(include_paths.iter().map(|path| format!("-I{}", path)))
        .clang_args(["-x", "c++", "-std=c++2a"])
        .enable_cxx_namespaces()
        .newtype_enum(".*")
        .allowlist_type(".*(Exception|Time|Status|StreamType|MessageType|InputBuffer_2|EncryptionScheme|SubsampleEntry|Pattern|KeyInformation|InitDataType|SessionType|QueryResult|OutputLinkTypes|OutputProtectionMethods)")
        .blocklist_function(".*")
        .derive_default(true)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn main() {
    let include_paths = &["/home/dennis/workspace/dacha/third_party/chromium/cdm/repo"];

    run_bindgen(include_paths);

    cxx_build::bridge("src/ffi.rs") // returns a cc::Build
        .file("src/ffi.cc")
        .includes(include_paths)
        .flag("-std=c++2a")
        .flag("-Wall")
        .flag("-Wno-unused-variable")
        .flag("-Wno-unused-parameter")
        .compile("chromium_cdm-cxx");

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/ffi.c");
    println!("cargo:rerun-if-changed=src/ffi.h");

    println!("cargo:rerun-if-changed=src/bindings.h");

    println!("cargo:rustc-link-lib=dylib=widevinecdm");
    println!(
        "cargo:rustc-link-search=native=/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64"
    );

    // Where to search for the library at runtime.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64");

    //
}
