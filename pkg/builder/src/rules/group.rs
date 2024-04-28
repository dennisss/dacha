use common::errors::*;

use crate::label::Label;
use crate::proto::*;
use crate::rule::*;
use crate::target::*;
use crate::BuildConfigTarget;

pub struct GroupRule {
    attrs: GroupAttrs,
    config_label: Label,
}

impl BuildRule for GroupRule {
    type Attributes = GroupAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, context: &BuildConfigTarget) -> Result<Self::Target> {
        Ok(Self {
            attrs: attributes,
            config_label: context.label.clone(),
        })
    }
}

#[async_trait]
impl BuildTarget for GroupRule {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        let mut deps = BuildTargetDependencies::default();

        for label in self.attrs.deps() {
            deps.deps.insert(BuildTargetKey {
                label: Label::parse(label)?,
                config_label: self.config_label.clone(),
            });
        }

        Ok(deps)
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        let mut outputs = BuildTargetOutputs::default();

        for dep in self.attrs.deps() {
            let input_key = BuildTargetKey {
                label: Label::parse(dep)?,
                config_label: self.config_label.clone(),
            };

            let input = context
                .inputs
                .get(&input_key)
                .ok_or_else(|| err_msg("Missing input dependency"))?;

            outputs
                .output_files
                .extend(input.target_outputs.output_files.clone().into_iter());
        }

        Ok(outputs)
    }
}
