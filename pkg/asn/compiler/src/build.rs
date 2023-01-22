// Helpers for using in a build.rs package.

use std::env;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use common::errors::*;
use file::{LocalPath, LocalPathBuf};

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
    let input_dir = file::current_dir()?;

    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR")?);

    build_in_directory(&input_dir, &output_dir)
}

pub fn build_in_directory(input_dir: &LocalPath, output_dir: &LocalPath) -> Result<()> {
    let mut tracer = Tracer::new();

    let mut compiler = Arc::new(Compiler::new());

    let mut threads = vec![];

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.
    file::recursively_list_dir(&input_dir.join("src"), &mut |path: &LocalPath| {
        if path.extension().unwrap_or_default() != "asn1" {
            return;
        }

        let input_path = path.to_owned();

        let relative_path = path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.as_str());

        // TODO: Only perform '-' to '_' on the base name
        let mut output_path = output_dir.join(relative_path.as_str().replace("-", "_"));
        output_path.set_extension("rs");

        let compiler = compiler.clone();
        threads.push(thread::spawn(move || {
            let mut tracer = Tracer::new();

            let r = compiler.add(input_path.clone(), output_path);

            tracer.trace(input_path.as_str());

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
