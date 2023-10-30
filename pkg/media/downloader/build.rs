fn main() {
    // Where to search for the library at runtime.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64");

    //
}
