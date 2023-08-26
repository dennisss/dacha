use common::errors::*;
use common::failure::ResultExt;
use file::LocalPath;
use protobuf::{text::*, Message};
use protobuf::{DescriptorPool, DynamicMessage};

use crate::proto::*;
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
        let mut data = file::read_to_string(context.package_dir.join(self.attrs.src())).await?;

        let mut file = TextMessageFile::parse(&data)
            .with_context(|e| format_err!("While parsing {}: {}", self.attrs.src(), data))?;

        let proto_message = file
            .proto_message()
            .ok_or_else(|| err_msg("Textproto not annotated with a message type"))?;
        let proto_file = file
            .proto_file()
            .ok_or_else(|| err_msg("Textproto not annotated with a message file"))?;

        let mut descriptor_pool = DescriptorPool::new();

        // TODO: Verify this file contains the proto (and not one of its imports or
        // dependencies).
        let package = descriptor_pool
            .add_proto_file(
                context.workspace_dir.join(proto_file),
                &context.workspace_dir,
            )
            .await?;

        // TODO: Switch to interpreting these as labels.
        for dep in self.attrs.deps() {
            descriptor_pool
                .add_proto_file(context.workspace_dir.join(dep), &context.workspace_dir)
                .await?;
        }

        let message_type = descriptor_pool
            .find_relative_type(&package, &proto_message)
            .ok_or_else(|| format_err!("Failed to find type: {}", proto_message))?
            .to_message()
            .ok_or_else(|| format_err!("Type is not a message: {}", proto_message))?;

        let mut proto = DynamicMessage::new(message_type);

        file.merge_to(
            &mut proto,
            &ParseTextProtoOptions {
                extension_handler: Some(&descriptor_pool),
            },
        )
        .with_context(|e| format_err!("While merging file {}: {}", self.attrs.src(), e))?;

        let mut output_filename = LocalPath::new(self.attrs.src()).to_owned();
        output_filename.set_extension("binaryproto"); // TODO: assert wrap here.

        let mut output_key = LocalPath::new(&context.key.label.directory)
            .join(&output_filename)
            .as_str()
            .to_string();

        let mut output_path = context
            .workspace_dir
            .join("built-config")
            .join(&context.config_hash)
            .join(&output_key);

        file::create_dir_all(output_path.parent().unwrap()).await?;

        file::write(&output_path, &proto.serialize()?).await?;

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
