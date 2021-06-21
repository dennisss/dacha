use crate::compiler::Compiler;
use crate::syntax::parse_proto;
use common::errors::*;
use std::env;
use std::fs::DirEntry;
use std::path::PathBuf;

// TODO: Most of this code is identical across the different implements and could probably be refactored out!!
pub fn build() -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = env::current_dir()?;

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    let mut input_paths: Vec<PathBuf> = vec![];

    let runtime_package = {
        if input_dir.file_name().unwrap() == "protobuf" {
            "crate"
        } else {
            "protobuf"
        }
    };
    

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

    for input_path in input_paths {
        let relative_path = input_path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.to_str().unwrap());

        let input_src = std::fs::read_to_string(input_path)?;

        let desc = match parse_proto(&input_src) {
            Ok(d) => d,
            Err(e) => {
                return Err(format_err!("Failed to parse {:?}: {:?}", relative_path, e));
            }
        };

        let mut output_path = output_dir.join(relative_path);
        output_path.set_extension("rs");

        std::fs::create_dir_all(output_path.parent().unwrap())?;

        let output = Compiler::compile(&desc, runtime_package)?;
        std::fs::write(output_path, output)?;
    }

    Ok(())
}
