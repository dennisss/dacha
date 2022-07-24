use common::async_std::fs;
use common::async_std::path::{Path, PathBuf};
use common::errors::*;
use common::failure::ResultExt;
use protobuf::DescriptorPool;
use protobuf::{text::*, Message};

use crate::proto::config::BuildConfig;
use crate::proto::rule::*;
use crate::rule::*;
use crate::target::*;

pub struct ProtoData {
    attrs: ProtoDataAttrs,
}

impl BuildRule for ProtoData {
    type Attributes = ProtoDataAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self { attrs: attributes })
    }
}

#[async_trait]
impl BuildTarget for ProtoData {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        // TODO: Depends on all files (they must all by loaded into the environment to
        // be useable).
        Ok(BuildTargetDependencies::default())
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        let mut data = fs::read_to_string(context.package_dir.join(self.attrs.src())).await?;

        let mut file = TextMessageFile::parse(&data)
            .with_context(|e| format_err!("While parsing {}: {}", self.attrs.src(), data))?;

        let mut descriptor_pool = DescriptorPool::new();

        // TODO: Stop hard coding this.
        descriptor_pool
            .add_local_file(project_path!("pkg/builder/src/proto/config.proto"))
            .await?;
        descriptor_pool
            .add_local_file(project_path!("pkg/builder/src/proto/rule.proto"))
            .await?;

        let mut proto = BuildConfig::default();

        file.merge_to(
            &mut proto,
            &ParseTextProtoOptions {
                extension_handler: Some(&descriptor_pool),
            },
        )
        .with_context(|e| format_err!("While merging file {}: {}", self.attrs.src(), e))?;

        let mut output_filename = Path::new(self.attrs.src()).to_owned();
        assert!(output_filename.set_extension("binaryproto"));

        let mut output_key = Path::new(&context.key.label.directory)
            .join(&output_filename)
            .to_str()
            .unwrap()
            .to_string();

        let mut output_path = context
            .workspace_dir
            .join("built-config")
            .join(&context.config_hash)
            .join(&output_key);

        fs::create_dir_all(output_path.parent().unwrap()).await?;

        fs::write(&output_path, &proto.serialize()?).await?;

        let mut outputs = BuildTargetOutputs::default();

        outputs.output_files.insert(
            output_key,
            BuildOutputFile {
                location: output_path,
            },
        );

        Ok(outputs)
    }
}
