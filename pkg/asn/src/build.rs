// Helpers for using in a build.rs package.

use std::env;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use common::errors::*;

use crate::compiler::Compiler;

pub struct Tracer {
    last_time: Instant,
}

impl Tracer {
    fn new() -> Self {
        Self {
            last_time: Instant::now(),
        }
    }

    fn trace(&mut self, event_name: &str) {
        let time = Instant::now();
        let dur = time - self.last_time;

        println!("[Trace] {} : {}ms", event_name, dur.as_millis() as u32);
        self.last_time = time;
    }
}

/// Call in the build.rs script of a package to compile all ASN files to Rust
/// code.
pub fn build() -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = env::current_dir()?;

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);

    build_in_directory(&input_dir, &output_dir)
}

pub fn build_in_directory(input_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut tracer = Tracer::new();

    let mut compiler = Arc::new(Compiler::new());

    let mut threads = vec![];

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

        let input_path = entry.path();

        let relative_path = entry.path().strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.to_str().unwrap());

        // TODO: Only perform '-' to '_' on the base name
        let mut output_path = output_dir.join(relative_path.to_str().unwrap().replace("-", "_"));
        output_path.set_extension("rs");

        let compiler = compiler.clone();
        threads.push(thread::spawn(move || {
            let mut tracer = Tracer::new();

            let r = compiler.add(input_path.clone(), output_path);

            tracer.trace(input_path.to_str().unwrap());

            r
        }));
    })?;

    tracer.trace("Files Listed");

    for thread in threads {
        thread.join().unwrap().unwrap();
    }

    tracer.trace("Parsing Done");

    compiler.compile_all()?;

    tracer.trace("Compilation Done");

    Ok(())
}
