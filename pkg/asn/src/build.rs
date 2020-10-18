// Helpers for using in a build.rs package.

use crate::compiler::Compiler;
use common::errors::*;
use std::env;
use std::fs::DirEntry;
use std::path::PathBuf;

/// Call in the build.rs script of a package to compile all ASN files to Rust
/// code.
pub fn build() -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = env::current_dir()?;

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);

    let mut compiler = Compiler::new();

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.
    common::fs::recursively_list_dir(&input_dir.join("src"), &mut |entry: &DirEntry| {
        if entry
            .path()
            .extension()
            .unwrap_or(std::ffi::OsStr::new(""))
            .to_str()
            .unwrap()
            != "asn1"
        {
            return;
        }

        let relative_path = entry.path().strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.to_str().unwrap());

        // TODO: Only perform '-' to '_' on the base name
        let mut output_path = output_dir.join(relative_path.to_str().unwrap().replace("-", "_"));
        output_path.set_extension("rs");

        compiler.add(entry.path(), output_path).unwrap();
    })?;

    compiler.compile_all()?;

    Ok(())
}
