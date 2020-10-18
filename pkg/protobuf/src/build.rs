use crate::compiler::Compiler;
use crate::syntax::proto;
use common::errors::*;
use std::env;
use std::fs::DirEntry;
use std::path::PathBuf;

pub fn build() -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = env::current_dir()?;

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    let mut input_paths = vec![];

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

        let (desc, rest) = match proto(&input_src) {
            Ok(d) => d,
            Err(e) => {
                println!("{:?}", e);
                return Ok(());
            }
        };

        if rest.len() != 0 {
            println!("Not parsed till end! {:?}", rest);
            return Ok(());
        }

        let mut output_path = output_dir.join(relative_path);
        output_path.set_extension("rs");

        std::fs::create_dir_all(output_path.parent().unwrap());

        let output = Compiler::compile(&desc);
        std::fs::write(output_path, output)?;
    }

    Ok(())
}
