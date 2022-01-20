use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::process::{Command, Stdio};

use common::async_std::fs;
use common::async_std::os::unix::fs::symlink;
use common::async_std::path::{Path, PathBuf};
use common::failure::ResultExt;
use common::{errors::*, project_dir};
use compression::tar::{AppendFileOptions, FileMetadataMask};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use protobuf::Message;

use crate::context::*;
use crate::label::*;
use crate::proto::bundle::*;
use crate::proto::config::*;
use crate::target::*;

pub struct Builder {
    // TODO: Could make the option tri-state:
    // We are always either:
    // 1. Enqueing,
    // 2. Running, or
    // 3. Completed.
    built_targets: HashMap<BuildResultKey, Option<BuildResult>>,
    workspace_dir: PathBuf,
    output_dir: PathBuf,
}

impl Builder {
    pub fn default() -> Self {
        let workspace_dir = PathBuf::from(common::project_dir());
        Self::new(&workspace_dir)
    }

    pub fn new(workspace_dir: &Path) -> Self {
        Self {
            built_targets: HashMap::new(),
            workspace_dir: workspace_dir.to_path_buf(),
            output_dir: workspace_dir.join("built"),
        }
    }

    pub async fn build_target_cwd(
        &mut self,
        label: &str,
        context: &BuildContext,
    ) -> Result<BuildResult> {
        let current_dir = PathBuf::from(std::env::current_dir()?);
        if !current_dir.starts_with(&self.workspace_dir) {
            return Err(err_msg("Must run the builder from inside a workspace"));
        }

        self.build_target(label, Some(&current_dir), context).await
    }

    pub async fn build_target(
        &mut self,
        label: &str,
        current_dir: Option<&Path>,
        context: &BuildContext,
    ) -> Result<BuildResult> {
        let (label, target_dir, spec) = self.lookup_target(label, current_dir).await?;

        let result_key = BuildResultKey {
            label,
            config_key: context.config_key.clone(),
        };

        if let Some(existing_result) = self.built_targets.get(&result_key) {
            match existing_result {
                Some(result) => {
                    return Ok(result.clone());
                }
                None => {
                    return Err(err_msg("Target already being built. Recursive loop?"));
                }
            }
        }

        // Mark that it is currently been built to prevent recursive looks.
        self.built_targets.insert(result_key.clone(), None);

        for dep in spec.deps() {
            self.build_target_recurse(dep.as_str(), Some(&target_dir), context)
                .await?;
        }

        let result = self
            .build_single_target(result_key, &spec, &target_dir, context)
            .await?;

        self.built_targets
            .insert(result.key.clone(), Some(result.clone()));

        Ok(result)
    }

    /// Invokes self.build_target(). Can be used inside of functions called by
    /// build_target().
    fn build_target_recurse<'a>(
        &'a mut self,
        label: &'a str,
        current_dir: Option<&'a Path>,
        context: &'a BuildContext,
    ) -> Pin<Box<dyn Future<Output = Result<BuildResult>> + 'a>> {
        Box::pin(self.build_target(label, current_dir, context))
    }

    async fn lookup_target(
        &self,
        label: &str,
        current_dir: Option<&Path>,
    ) -> Result<(Label, PathBuf, BuildTarget)> {
        let mut label = Label::parse(label)?;

        // Make the label absolute.
        if !label.absolute {
            let current_dir = current_dir.ok_or_else(|| {
                format_err!(
                    "Not building in a specific directory. Must specify an absolute label: {:?}",
                    label
                )
            })?;

            label.directory = current_dir
                .strip_prefix(&&self.workspace_dir)
                .unwrap()
                .join(&label.directory)
                .to_str()
                .unwrap()
                .to_string();
            label.absolute = true;
        }

        // Directory in which the BUILD file for the required
        let target_dir = self.workspace_dir.join(&label.directory);

        let build_file_path = target_dir.join("BUILD");
        if !build_file_path.exists().await {
            return Err(format_err!("Missing build file at: {:?}", build_file_path));
        }

        let build_file_data = fs::read_to_string(&build_file_path).await?;

        // TODO: Cache these in the builder instance.
        let mut build_file = BuildFile::default();
        protobuf::text::parse_text_proto(&build_file_data, &mut build_file)
            .with_context(|e| format!("While parsing {:?}: {}", build_file_path, e))?;

        let mut targets = HashMap::new();
        for target in BuildTarget::list_all(&build_file).into_iter() {
            if targets
                .insert(target.name().to_string(), target.clone())
                .is_some()
            {
                return Err(format_err!("Duplicate target named: {}", target.name()));
            }
        }

        let spec = match targets.get(label.target_name.as_str()) {
            Some(v) => v,
            None => {
                return Err(format_err!(
                    "Failed to find target named: '{}' in dir '{}'",
                    label.target_name.as_str(),
                    target_dir.to_str().unwrap()
                ));
            }
        };

        Ok((label, target_dir, spec.clone()))
    }

    async fn build_single_target(
        &mut self,
        key: BuildResultKey,
        spec: &BuildTarget,
        target_dir: &Path,
        context: &BuildContext,
    ) -> Result<BuildResult> {
        // TODO: We need to be diligent about removing old files if a target is rebuilt.

        let mut result = BuildResult {
            key: key.clone(),
            output_files: HashMap::new(),
        };

        match &spec.raw {
            BuildTargetRaw::RustBinary(raw_target) => {
                let mut target = context.config.rust_binary().clone();
                target.merge_from(&raw_target)?;

                // NOTE: We assume that the name of the rust package is the same as the name of
                // the directory in which the BUILD file is located.
                let package_name = target_dir.file_name().unwrap().to_str().unwrap();

                let bin_name = if target.name() == "main" {
                    package_name
                } else if !target.bin().is_empty() {
                    target.bin()
                } else {
                    target.name()
                };

                let rust_target_dir = self
                    .workspace_dir
                    .join("built-rust")
                    .join(&context.config_key);
                // NOTE: we must create the directory otherwise 'cross' tends to screw up the
                // permissions and make root the owner of the directory.
                fs::create_dir_all(&rust_target_dir).await?;

                // Add --target-dir when using cross.

                let program = match target.compiler() {
                    RustCompiler::UNKNOWN | RustCompiler::CARGO => "cargo",
                    RustCompiler::CROSS => "cross",
                };

                let mut cmd = Command::new(program);

                cmd.arg("build")
                    .arg("--package")
                    .arg(package_name)
                    .arg("--bin")
                    .arg(bin_name)
                    .arg("--target-dir")
                    .arg(rust_target_dir.to_str().unwrap())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());

                let mut effective_profile = "debug";
                match target.profile() {
                    "" => {}
                    "release" => {
                        cmd.arg("--release");
                        effective_profile = "release";
                    }
                    profile @ _ => {
                        return Err(format_err!("Unsupported rust profile: {}", profile))
                    }
                };

                // TODO: Assert this is always
                if !target.target().is_empty() {
                    cmd.arg("--target");
                    cmd.arg(target.target());
                }

                let mut child = cmd.spawn()?;

                let status = child.wait()?;
                if !status.success() {
                    return Err(format_err!("cargo failed with status: {:?}", status));
                }

                let binary_path = rust_target_dir
                    .join(target.target())
                    .join(effective_profile)
                    .join(bin_name);

                let mount_path = Path::new("built")
                    .join(&key.label.directory)
                    .join(&key.label.target_name)
                    .to_str()
                    .unwrap()
                    .to_string();

                result.output_files.insert(mount_path, binary_path);
            }
            BuildTargetRaw::FileGroup(_) => {
                // Nothing to do. Maybe just verify that all the files exist?
            }
            BuildTargetRaw::Bundle(target) => {
                let bundle_mount_dir = Path::new("built")
                    .join(&key.label.directory)
                    .join(&key.label.target_name);

                let bundle_dir = self
                    .workspace_dir
                    .join("built-config")
                    .join(&context.config_key)
                    .join(&key.label.directory)
                    .join(&key.label.target_name);
                if bundle_dir.exists().await {
                    // TODO: We may need to use the regular remove_file function if it was
                    // originally a file.
                    fs::remove_dir_all(&bundle_dir).await?;
                }
                fs::create_dir_all(&bundle_dir).await?;

                let mut bundle_spec = BundleSpec::default();

                if target.configs_len() == 0 {
                    return Err(err_msg("Bundle must define at least one config"));
                }

                for config in target.configs() {
                    let sub_context =
                        BuildContext::from(self.lookup_config(config, Some(target_dir)).await?)?;

                    let mut combined_outputs = HashMap::new();

                    for dep in target.deps() {
                        let res = self
                            .build_target_recurse(dep, Some(target_dir), &sub_context)
                            .await?;
                        combined_outputs.extend(res.output_files);
                    }

                    // Temporary path to which we'll write the archive before we know the hash of
                    // the file.
                    let archive_path = bundle_dir.join("archive.tar");
                    let mut out = compression::tar::Writer::open(&archive_path).await?;

                    // Add all files to the archive.
                    // NOTE: A current limitation is that because BuildResult only lists files, we
                    // don't preserve any directory metadata.
                    for src in target.absolute_srcs() {
                        // TODO: Verify that all of the 'absolute_srcs' are relative paths.

                        let path = combined_outputs
                            .get(src)
                            .ok_or_else(|| format_err!("Missing build output for: {}", src))?;

                        let options = AppendFileOptions {
                            root_dir: path.clone(),
                            output_dir: Some(src.into()),
                            mask: FileMetadataMask {},
                            anonymize: true,
                        };
                        out.append_file(path, &options).await?;
                    }

                    // TODO: Given the entire archive will be passing through memory, can we hash it
                    // while we are writing it to disk?
                    out.finish().await?;

                    let blob_spec = {
                        let data = fs::read(&archive_path).await?;

                        let hash = {
                            let mut hasher = SHA256Hasher::default();
                            let hash = hasher.finish_with(&data);
                            format!("sha256:{}", common::hex::encode(hash))
                        };

                        let mut spec = BlobSpec::default();
                        spec.set_id(hash);
                        spec.set_size(data.len() as u64);
                        spec.set_format(BlobFormat::TAR_ARCHIVE);
                        spec
                    };

                    // Move to final location
                    let blob_path = bundle_dir.join(blob_spec.id());
                    fs::rename(archive_path, &blob_path).await?;

                    result.output_files.insert(
                        bundle_mount_dir
                            .join(blob_spec.id())
                            .to_str()
                            .unwrap()
                            .to_string(),
                        blob_path,
                    );

                    let mut variant = BundleVariant::default();
                    variant.set_platform(sub_context.config.platform().clone());
                    variant.set_blob(blob_spec);
                    bundle_spec.add_variants(variant);
                }

                let spec_path = bundle_dir.join("spec.textproto");
                let spec_mount_path = bundle_mount_dir.join("spec.textproto");

                fs::write(
                    &spec_path,
                    protobuf::text::serialize_text_proto(&bundle_spec),
                )
                .await?;

                result
                    .output_files
                    .insert(spec_mount_path.to_str().unwrap().to_string(), spec_path);
            }
            BuildTargetRaw::Webpack(spec) => {
                // TODO: Verify at most one webpack target is present per build directory.

                let output_mount_path = Path::new("built")
                    .join(&key.label.directory)
                    .join(format!("{}.js", key.label.target_name));

                let output_path = self
                    .workspace_dir
                    .join("built-config")
                    .join(&context.config_key)
                    .join(&key.label.directory)
                    .join(format!("{}.js", key.label.target_name));

                let bin = self.workspace_dir.join("node_modules/.bin/webpack");

                let mut child = Command::new(bin)
                    .arg("--config")
                    .arg(self.workspace_dir.join("pkg/web/webpack.config.js"))
                    .arg("--env")
                    .arg(&format!(
                        "entry={}",
                        target_dir.join(spec.entry()).to_str().unwrap()
                    ))
                    .arg("--env")
                    .arg(&format!("output={}", output_path.to_str().unwrap()))
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()?;

                let status = child.wait()?;
                if !status.success() {
                    return Err(format_err!("Webpack failed: {:?}", status));
                }

                result
                    .output_files
                    .insert(output_mount_path.to_str().unwrap().to_string(), output_path);
            }
            BuildTargetRaw::BuildConfig(_) => {
                // Just used as metadata.
            }
        }

        Ok(result)
    }

    pub async fn lookup_config(
        &self,
        label: &str,
        current_dir: Option<&Path>,
    ) -> Result<BuildConfig> {
        let (label, _, config_target) = self.lookup_target(label, current_dir).await?;

        match config_target.raw {
            BuildTargetRaw::BuildConfig(c) => {
                return Ok(c);
            }
            _ => {
                return Err(format_err!(
                    "Target is not a build config: {}",
                    label.target_name
                ));
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BuildResultKey {
    /// NOTE: This will be absolute.
    pub label: Label,

    pub config_key: String,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub key: BuildResultKey,

    /// Absolute paths to files generated by directly building the requested
    /// target. This doesn't include any files linked as dependencies.
    pub output_files: HashMap<String, PathBuf>,
}

// #[derive(Debug, Clone)]
// pub struct OutputFile {
//     /// NOTE: Every single OutputFile must have a distinct mount_path. This
// must     /// also be unique across different rules.
//     pub mount_path: String,

//     pub source_path: PathBuf,
// }
