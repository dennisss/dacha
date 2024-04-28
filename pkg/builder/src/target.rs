use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use common::errors::*;
use file::LocalPathBuf;

use crate::context::BuildConfigTarget;
use crate::label::Label;

#[async_trait]
pub trait BuildTarget: 'static + Send + Sync {
    fn name(&self) -> &str;

    fn dependencies(&self) -> Result<BuildTargetDependencies>;

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs>;
}

#[derive(Default)]
pub struct BuildTargetDependencies {
    pub deps: HashSet<BuildTargetKey>,
}

pub struct BuildTargetContext {
    pub key: BuildTargetKey,
    pub config_hash: String,
    pub workspace_dir: LocalPathBuf,
    pub package_dir: LocalPathBuf,
    pub inputs: HashMap<BuildTargetKey, BuildTargetInput>,
}

pub struct BuildTargetInput {
    pub config: BuildConfigTarget,
    pub target_outputs: BuildTargetOutputs,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BuildTargetKey {
    /// NOTE: This will be absolute.
    pub label: Label,

    /// Absolute label identifying the configuration used to build the above
    /// rule.
    pub config_label: Label,
}

#[derive(Debug, Clone, Default)]
pub struct BuildTargetOutputs {
    /// Files produced by this target. This doesn't include any files linked as
    /// dependencies unless the target explicitly forwarded them.
    ///
    /// The key is the canonical name of the file (a path relative to the
    /// workspace root).
    ///
    /// TODO: Use an ordered hashmap for determinism.
    pub output_files: HashMap<String, BuildOutputFile>,
}

#[derive(Debug, Clone)]
pub struct BuildOutputFile {
    /// Location of disk where this file is located.
    pub location: LocalPathBuf,
}
