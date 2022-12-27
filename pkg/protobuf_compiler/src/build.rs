use std::io::Write;
use std::process::Command;

use common::errors::*;
use file::{LocalPath, LocalPathBuf};

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
    let input_dir = file::current_dir()?;

    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    let current_package_name = input_dir.file_name().unwrap();

    build_custom(&input_dir, &output_dir, current_package_name, options)
}

pub fn build_custom(
    input_dir: &LocalPath,
    output_dir: &LocalPath,
    current_package_name: &str,
    options: CompilerOptions,
) -> Result<()> {
    let mut input_paths: Vec<LocalPathBuf> = vec![];

    file::recursively_list_dir(&input_dir.join("src"), &mut |path: &LocalPath| {
        if path.extension().unwrap_or_default() != "proto" {
            return;
        }

        input_paths.push(path.to_owned());
    })?;

    // TODO: Parallelize this?
    for input_path in input_paths {
        let relative_path = input_path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.as_str());

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
            let res = Command::new("rustfmt").arg(output_path.as_str()).output()?;
            if !res.status.success() {
                std::io::stdout().write_all(&res.stdout).unwrap();
                std::io::stderr().write_all(&res.stderr).unwrap();
                return Err(err_msg("rustfmt failed"));
            }
        }
    }

    Ok(())
}
