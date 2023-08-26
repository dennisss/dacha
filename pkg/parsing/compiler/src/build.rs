use std::io::Write;

// use crate::syntax::parse_proto;
use common::errors::*;
use file::{LocalPath, LocalPathBuf};

use crate::compiler::Compiler;

pub fn build() -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = file::current_dir()?;

    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    let mut input_paths: Vec<LocalPathBuf> = vec![];

    file::recursively_list_dir(&input_dir.join("src"), &mut |path: &LocalPath| {
        if path.extension().unwrap_or_default() != "binproto" {
            return;
        }

        input_paths.push(path.to_owned());
    })?;

    for input_path in input_paths {
        let relative_path = input_path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.as_str());

        let input_src = std::fs::read_to_string(input_path)?;

        let mut lib = crate::proto::BinaryDescriptorLibrary::default();
        if let Err(e) = protobuf::text::parse_text_proto(&input_src, &mut lib) {
            return Err(format_err!("Failed to parse {:?}: {:?}", relative_path, e));
        }

        let mut output_path = output_dir.join(&relative_path);
        output_path.set_extension("rs");

        std::fs::create_dir_all(output_path.parent().unwrap())?;

        let output = Compiler::compile(lib)
            .map_err(|e| format_err!("While compiling {:?}: {}", relative_path, e))?;
        std::fs::write(&output_path, output)?;

        {
            // TODO: This doesn't work with 'cross'
            let res = std::process::Command::new("rustfmt")
                .arg(output_path.as_str())
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
