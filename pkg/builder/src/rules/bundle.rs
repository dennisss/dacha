use common::errors::*;
use compression::tar::{AppendFileOptions, FileMetadataMask};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use file::LocalPath;

use crate::label::Label;
use crate::proto::bundle::*;
use crate::proto::config::BuildConfig;
use crate::proto::rule::*;
use crate::rule::*;
use crate::target::*;

pub struct Bundle {
    attrs: BundleAttrs,
    platform: Platform,
}

impl BuildRule for Bundle {
    type Attributes = BundleAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self {
            attrs: attributes,
            platform: config.platform().clone(),
        })
    }
}

#[async_trait]
impl BuildTarget for Bundle {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        let mut deps = BuildTargetDependencies::default();

        for config in self.attrs.configs() {
            for dep in self.attrs.deps() {
                deps.deps.insert(BuildTargetKey {
                    label: Label::parse(dep)?,
                    config_label: Label::parse(config)?,
                });
            }
        }

        Ok(deps)
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        let mut outputs = BuildTargetOutputs::default();

        let bundle_mount_dir = LocalPath::new("built")
            .join(&context.key.label.directory)
            .join(&context.key.label.target_name);

        let bundle_dir = context
            .workspace_dir
            .join("built-config")
            .join(&context.config_hash)
            .join(&context.key.label.directory)
            .join(&context.key.label.target_name);
        if file::exists(&bundle_dir).await? {
            // TODO: We may need to use the regular remove_file function if it was
            // originally a file.
            file::remove_dir_all(&bundle_dir).await?;
        }
        file::create_dir_all(&bundle_dir).await?;

        let mut bundle_spec = BundleSpec::default();

        if self.attrs.configs_len() == 0 {
            return Err(err_msg("Bundle must define at least one config"));
        }

        for config in self.attrs.configs() {
            // Temporary path to which we'll write the archive before we know the hash of
            // the file.
            let archive_path = bundle_dir.join("archive.tar");
            let mut out = compression::tar::Writer::open(&archive_path).await?;

            // let mut combined_outputs = HashMap::new();

            for dep in self.attrs.deps() {
                let input_key = BuildTargetKey {
                    label: Label::parse(dep)?,
                    config_label: Label::parse(config)?,
                };

                let input = context
                    .inputs
                    .get(&input_key)
                    .ok_or_else(|| err_msg("Missing input dependency"))?;

                // Add all files to the archive.
                // NOTE: A current limitation is that because BuildResult only lists files,
                // we don't preserve any directory metadata.
                for (src, file) in &input.output_files {
                    let options = AppendFileOptions {
                        root_dir: file.location.clone(),
                        output_dir: Some(src.into()),
                        mask: FileMetadataMask {},
                        anonymize: true,
                    };
                    out.append_file(&file.location, &options).await?;
                }
            }

            // TODO: Given the entire archive will be passing through memory, can we hash it
            // while we are writing it to disk?
            out.finish().await?;

            let blob_spec = {
                let data = file::read(&archive_path).await?;

                let hash = {
                    let mut hasher = SHA256Hasher::default();
                    let hash = hasher.finish_with(&data);
                    format!("sha256:{}", radix::hex_encode(&hash))
                };

                let mut spec = BlobSpec::default();
                spec.set_id(hash);
                spec.set_size(data.len() as u64);
                spec.set_format(BlobFormat::TAR_ARCHIVE);
                spec
            };

            // Move to final location
            let blob_path = bundle_dir.join(blob_spec.id());
            file::rename(archive_path, &blob_path).await?;

            outputs.output_files.insert(
                bundle_mount_dir.join(blob_spec.id()).to_string(),
                BuildOutputFile {
                    location: blob_path,
                },
            );

            let mut variant = BundleVariant::default();
            variant.set_platform(self.platform.clone());
            variant.set_blob(blob_spec);
            bundle_spec.add_variants(variant);
        }

        let spec_path = bundle_dir.join("spec.textproto");
        let spec_mount_path = bundle_mount_dir.join("spec.textproto");

        let data = protobuf::text::serialize_text_proto(&bundle_spec);

        file::write(&spec_path, data).await?;

        outputs.output_files.insert(
            spec_mount_path.to_string(),
            BuildOutputFile {
                location: spec_path,
            },
        );

        Ok(outputs)
    }
}
