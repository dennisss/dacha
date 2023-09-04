use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;

use common::errors::*;
use common::line_builder::LineBuilder;
use file::{project_path, LocalPath, LocalPathBuf};
use protobuf_dynamic::DescriptorPool;

use crate::compiler::{Compiler, CompilerOptions};
use crate::escape::escape_rust_identifier;

pub fn project_default_options() -> CompilerOptions {
    let mut options = CompilerOptions::default();

    // TODO: Infer these from build dependencies.
    options
        .descriptor_pool_options
        .paths
        .push(project_path!("third_party/protobuf_builtins/proto"));
    options
        .descriptor_pool_options
        .paths
        .push(project_path!("third_party/protobuf_descriptor"));
    options
        .descriptor_pool_options
        .paths
        .push(project_path!("third_party/googleapis/repo"));

    // TODO: This is a dangerous default() as we can't use it outside of a real
    // project.
    options
        .descriptor_pool_options
        .paths
        .push(file::project_dir());

    options
}

// TODO: Most of this code is identical across the different implements and
// could probably be refactored out!!
pub fn build() -> Result<()> {
    build_with_options(project_default_options())
}

pub fn build_with_options(options: CompilerOptions) -> Result<()> {
    // NOTE: This must be the root path of the package (containing the Cargo.toml
    // and build.rs).
    let input_dir = file::current_dir()?;

    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR")?);

    // TODO: How do we indicate that the directory could change (adding new files).

    // TODO: Propagate out the Results from inside the callback.

    build_custom(&input_dir, &output_dir, options)
}

#[derive(Default)]
struct PackageTree {
    files: Vec<(LocalPathBuf, String)>,
    child_packages: BTreeMap<String, PackageTree>,
}

impl PackageTree {
    fn insert(&mut self, package: &str, file: LocalPathBuf, file_id: String) {
        let mut package_path: Vec<&str> = package.split('.').collect::<Vec<_>>();
        if package.is_empty() {
            package_path.clear();
        }

        self.insert_with_path(&package_path[..], file, file_id)
    }

    fn insert_with_path(&mut self, package_path: &[&str], file: LocalPathBuf, file_id: String) {
        if package_path.is_empty() {
            self.files.push((file, file_id));
            return;
        }

        let inner = self
            .child_packages
            .entry(package_path[0].to_string())
            .or_default();
        inner.insert_with_path(&package_path[1..], file, file_id)
    }

    fn to_module(&self, options: &CompilerOptions, lines: &mut LineBuilder) {
        // lines.add("#![allow(dead_code, non_snake_case, unused_imports,
        // unused_variables)]");

        for (key, value) in self.child_packages.iter() {
            lines.add(format!("pub mod {} {{", escape_rust_identifier(key)));
            value.to_module(options, lines);
            lines.add("}");
        }

        for (file, file_id) in &self.files {
            lines.add(format!(
                r#"
                mod file_{file_id} {{
                    include!(concat!(env!("OUT_DIR"), "/{file}"));
                }}

                pub use self::file_{file_id}::*;

                "#,
                file_id = file_id.to_ascii_lowercase(),
                file = file.as_str()
            ));
        }
    }
}

pub fn build_custom(
    input_dir: &LocalPath,
    output_dir: &LocalPath,
    options: CompilerOptions,
) -> Result<()> {
    executor::run(build_custom_impl(input_dir, output_dir, options))?
}

async fn build_custom_impl(
    input_dir: &LocalPath,
    output_dir: &LocalPath,
    options: CompilerOptions,
) -> Result<()> {
    let mut input_paths: Vec<LocalPathBuf> = vec![];

    // TODO: Only traverse down directories accepted by the allowlist.
    file::recursively_list_dir(&input_dir, &mut |path: &LocalPath| {
        if path.extension().unwrap_or_default() != "proto" {
            return;
        }

        if let Some(allowlisted_paths) = &options.allowlisted_paths {
            let mut allowed = false;
            for p in allowlisted_paths {
                if path.starts_with(p) {
                    allowed = true;
                    break;
                }
            }

            if !allowed {
                return;
            }
        }

        input_paths.push(path.to_owned());
    })?;

    let mut tree = PackageTree::default();

    let mut descriptor_pool = DescriptorPool::new(options.descriptor_pool_options.clone());

    // TODO: Parallelize this?
    for input_path in input_paths {
        let mut relative_path = input_path.strip_prefix(&input_dir).unwrap().to_owned();
        println!("cargo:rerun-if-changed={}", relative_path.as_str());

        let file = descriptor_pool.add_file(&input_path).await?;

        relative_path.set_extension("rs");
        let mut output_path: LocalPathBuf = output_dir.join(&relative_path);

        std::fs::create_dir_all(output_path.parent().unwrap())?;

        let (output, file_id) = Compiler::compile(file.clone(), input_dir, &options)?;
        std::fs::write(&output_path, output)?;

        tree.insert(&file.proto().package(), relative_path, file_id);

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

    let mut lines = LineBuilder::new();

    tree.to_module(&options, &mut lines);

    let mod_path = output_dir.join("proto_lib.rs");
    std::fs::write(&mod_path, lines.to_string())?;

    Ok(())
}
