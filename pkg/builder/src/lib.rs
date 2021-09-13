/*
How bundle will work:
- Compile all dependencies.
- Take all absolute_srcs and
- Will be moved into build/pkg/sensor_monitor/bundle.tar
    - Mapped to 'pkg/sensor_monitor/bundle.tar' if used in the future

build-out/

*/

extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;

mod proto;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::process::{Command, Stdio};

use common::async_std::fs;
use common::async_std::path::{Path, PathBuf};
use common::errors::*;
use compression::tar::{AppendFileOptions, FileMetadataMask};

use crate::proto::config::*;

#[derive(Args)]
struct Args {
    command: ArgCommand,
}

#[derive(Args)]
enum ArgCommand {
    #[arg(name = "build")]
    Build(BuildCommand),
}

#[derive(Args)]
struct BuildCommand {
    #[arg(positional)]
    target: String,
}

#[derive(Debug)]
struct TargetPath {
    absolute: bool,
    directory: String,
    name: String,
}

#[derive(Clone, Copy)]
struct BuildTarget<'a> {
    name: &'a str,
    deps: &'a [String],
    raw: BuildTargetRaw<'a>,
}

#[derive(Clone, Copy)]
enum BuildTargetRaw<'a> {
    Bundle(&'a Bundle),
    RustBinary(&'a RustBinary),
    FileGroup(&'a FileGroup),
    Webpack(&'a Webpack),
}

impl<'a> BuildTarget<'a> {
    fn list_all(file: &BuildFile) -> Vec<BuildTarget> {
        let mut out = vec![];

        for raw in file.file_group() {
            out.push(BuildTarget {
                name: raw.name(),
                deps: raw.deps(),
                raw: BuildTargetRaw::FileGroup(raw),
            });
        }

        for raw in file.rust_binary() {
            out.push(BuildTarget {
                name: raw.name(),
                deps: raw.deps(),
                raw: BuildTargetRaw::RustBinary(raw),
            });
        }

        for raw in file.bundle() {
            out.push(BuildTarget {
                name: raw.name(),
                deps: raw.deps(),
                raw: BuildTargetRaw::Bundle(raw),
            });
        }

        for raw in file.webpack() {
            out.push(BuildTarget {
                name: raw.name(),
                deps: raw.deps(),
                raw: BuildTargetRaw::Webpack(raw),
            });
        }

        out
    }
}

fn parse_target_path(mut target: &str) -> Result<TargetPath> {
    let absolute = if let Some(t) = target.strip_prefix("//") {
        target = t;
        true
    } else {
        false
    };

    let (dir, name) = target
        .split_once(':')
        .ok_or_else(|| err_msg("Expected a : in the target path"))?;

    if dir.starts_with("/") {
        return Err(err_msg("Invalid directory in target path"));
    }

    Ok(TargetPath {
        absolute,
        directory: dir.to_string(),
        name: name.to_string(),
    })
}

struct Builder {
    built_targets: HashSet<String>,
    workspace_dir: PathBuf,
    output_dir: PathBuf,
}

impl Builder {
    async fn build_target(&mut self, target: &str, current_dir: &Path) -> Result<()> {
        let target = parse_target_path(target)?;
        let target_dir = {
            if target.absolute {
                self.workspace_dir.join(&target.directory)
            } else {
                current_dir.join(&target.directory)
            }
        };

        println!("TGT DIR: '{}'", target.directory);

        let target_key = format!("{}:{}", target_dir.to_str().unwrap(), target.name);
        if !self.built_targets.insert(target_key) {
            // Target was already built.
            return Ok(());
        }

        // TODO: Here we should acquire a lock / check if it has already been built.

        let build_file_path = target_dir.join("BUILD");
        if !build_file_path.exists().await {
            return Err(format_err!("Missing build file at: {:?}", build_file_path));
        }

        let build_file_data = fs::read_to_string(build_file_path).await?;

        let mut build_file = proto::config::BuildFile::default();
        protobuf::text::parse_text_proto(&build_file_data, &mut build_file)?;

        let mut targets = HashMap::new();
        for target in BuildTarget::list_all(&build_file).into_iter() {
            if targets.insert(target.name, target).is_some() {
                return Err(format_err!("Duplicate target named: {}", target.name));
            }
        }

        let spec = match targets.get(&target.name.as_str()) {
            Some(v) => v,
            None => {
                return Err(format_err!(
                    "Failed to find target named: '{}' in dir '{}'",
                    target.name.as_str(),
                    target_dir.to_str().unwrap()
                ));
            }
        };

        for dep in spec.deps {
            self.build_target_recurse(dep.as_str(), &target_dir).await?;
        }

        self.build_single_target(spec, &target_dir).await?;

        Ok(())
    }

    async fn build_single_target(
        &mut self,
        spec: &BuildTarget<'_>,
        target_dir: &Path,
    ) -> Result<()> {
        // TODO: We need to be diligent about removing old files if a target is rebuilt.

        match &spec.raw {
            BuildTargetRaw::RustBinary(spec) => {
                // NOTE: We assume that the name of the rust package is the same as the name of
                // the directory in which the BUILD file is located.
                let package_name = target_dir.file_name().unwrap().to_str().unwrap();

                let bin_name = if spec.name() == "main" {
                    package_name
                } else {
                    spec.name()
                };

                let mut child = Command::new("/home/dennis/.cargo/bin/cargo")
                    .arg("build")
                    .arg("--package")
                    .arg(package_name)
                    .arg("--bin")
                    .arg(bin_name)
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()?;

                let status = child.wait()?;
                if !status.success() {
                    return Err(format_err!("cargo failed with status: {:?}", status));
                }
            }
            BuildTargetRaw::FileGroup(_) => {
                // Nothing to do. Maybe just verify that all the files exist?
            }
            BuildTargetRaw::Bundle(spec) => {
                let out_file = self
                    .output_dir
                    .join(target_dir.strip_prefix(&self.workspace_dir).unwrap())
                    .join(format!("{}.tar", spec.name()));

                fs::create_dir_all(out_file.parent().unwrap()).await?;

                let mut out = compression::tar::Writer::open(out_file).await?;

                let options = AppendFileOptions {
                    root_dir: self.workspace_dir.clone(),
                    mask: FileMetadataMask {},
                };

                for src in spec.absolute_srcs() {
                    // TODO: Verify that all of the 'absolute_srcs' are relative paths.
                    out.append_file(&self.workspace_dir.join(src), &options)
                        .await?;
                }

                out.finish().await?;

                // Take all of the sources and put them into a tar file
            }
            BuildTargetRaw::Webpack(spec) => {
                // TODO: Verify at most one webpack target is present per build directory.

                let bin = self.workspace_dir.join("node_modules/.bin/webpack");

                let mut child = Command::new(bin)
                    .arg("-c")
                    .arg(target_dir.join("webpack.config.js"))
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()?;

                let status = child.wait()?;
                if !status.success() {
                    return Err(format_err!("Webpack failed: {:?}", status));
                }
            }
        }

        Ok(())
    }

    fn build_target_recurse<'a>(
        &'a mut self,
        target: &'a str,
        current_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(self.build_target(target, current_dir))
    }
}

async fn run_build(target: &str) -> Result<()> {
    let workspace_dir = PathBuf::from(common::project_dir());

    let mut builder = Builder {
        built_targets: HashSet::new(),
        workspace_dir: workspace_dir.clone(),
        output_dir: workspace_dir.join("build-out"),
    };

    let current_dir = PathBuf::from(std::env::current_dir()?);
    if !current_dir.starts_with(workspace_dir) {
        return Err(err_msg("Must run the builder from inside a workspace"));
    }

    builder.build_target(target, &current_dir).await?;

    Ok(())
}

pub fn run() -> Result<()> {
    common::async_std::task::block_on(async {
        let args = common::args::parse_args::<Args>()?;
        match args.command {
            ArgCommand::Build(build) => {
                run_build(&build.target).await?;
            }
        }

        Ok(())
    })
}
