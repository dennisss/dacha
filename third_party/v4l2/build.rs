#[macro_use]
extern crate regexp_macros;

use std::path::PathBuf;

// Matches a line like the following:
// #define V4L2_PIX_FMT_RGB332  v4l2_fourcc('R', 'G', 'B', '1')
regexp!(DEFINE_FOURCC_LINE => "^#define\\s+([^\\s]+)\\s+v4l2_fourcc(_be)?\\(([^)]+)\\)");

fn main() {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let bindings = bindgen::Builder::default()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .header("src/wrapper.h")
        .derive_debug(true)
        .derive_default(true)
        .newtype_enum(".*")
        .blocklist_function(".*")
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    // TODO: Remove the remainder of this logic once https://github.com/rust-lang/rust-bindgen/issues/753 is fixed.

    // TODO: Use the system specific one here.
    let header = std::fs::read_to_string("/usr/include/linux/videodev2.h").unwrap();

    let mut output = String::new();

    for line in header.lines() {
        if let Some(m) = DEFINE_FOURCC_LINE.exec(line) {
            let name = m.group_str(1).unwrap().unwrap();
            let be = m.group_str(2).map(|s| s.unwrap()).unwrap_or("");
            let chars = m.group_str(3).unwrap().unwrap();

            output.push_str(&format!(
                "pub const {}: u32 = v4l2_fourcc{}({});",
                name, be, chars
            ));
        }
    }

    std::fs::write(out_path.join("formats.rs"), output).unwrap();

    // println!("{:?}", out_path);
    // todo!();
}
