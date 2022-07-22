use std::env;
use std::fs::DirEntry;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::errors::*;

use crate::compiler::{Compiler, CompilerOptions};
use crate::syntax::parse_proto;

// TODO: Most of this code is identical across the different implements and
// could probably be refactored out!!
pub fn build() -> Result<()> {
    build_with_options(CompilerOptions::default())
}

pub fn build_with_options(options: CompilerOptions) -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = env::current_dir()?;

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    let current_package_name = input_dir.file_name().unwrap().to_str().unwrap();

    build_custom(&input_dir, &output_dir, current_package_name, options)
}

pub fn build_custom(
    input_dir: &Path,
    output_dir: &Path,
    current_package_name: &str,
    options: CompilerOptions,
) -> Result<()> {
    let mut input_paths: Vec<PathBuf> = vec![];

    common::fs::recursively_list_dir(&input_dir.join("src"), &mut |entry: &DirEntry| {
        if entry
            .path()
            .extension()
            .unwrap_or(std::ffi::OsStr::new(""))
            .to_str()
            .unwrap()
            != "proto"
        {
            return;
        }

        input_paths.push(entry.path().clone());
    })?;

    // TODO: Parallelize this?
    for input_path in input_paths {
        let relative_path = input_path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.to_str().unwrap());

        let input_src = std::fs::read_to_string(&input_path)?;

        let desc = match parse_proto(&input_src) {
            Ok(d) => d,
            Err(e) => {
                return Err(format_err!("Failed to parse {:?}: {:?}", relative_path, e));
            }
        };

        let mut output_path = output_dir.join(relative_path);
        output_path.set_extension("rs");

        std::fs::create_dir_all(output_path.parent().unwrap())?;

        let output = Compiler::compile(&desc, &input_path, current_package_name, &options)?;
        std::fs::write(&output_path, output)?;

        if options.should_format {
            // TODO: This doesn't work with 'cross'
            let res = Command::new("rustfmt")
                .arg(output_path.to_str().unwrap())
                .output()?;
            if !res.status.success() {
                std::io::stdout().write_all(&res.stdout).unwrap();
                std::io::stderr().write_all(&res.stderr).unwrap();
                return Err(err_msg("rustfmt failed"));
            }
        }
    }

    Ok(())
}
