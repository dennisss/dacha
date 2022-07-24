use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::Arc;

use common::async_std::fs;
use common::async_std::os::unix::fs::symlink;
use common::async_std::path::{Path, PathBuf};
use common::failure::ResultExt;
use common::{errors::*, project_dir};
use protobuf::{Message, StaticMessage};

use crate::context::*;
use crate::label::*;
use crate::package::*;
use crate::proto::bundle::*;
use crate::proto::config::*;
use crate::rule::BuildRuleRegistry;
use crate::target::*;
use crate::NATIVE_CONFIG_LABEL;

// TODO: We need to be diligent about removing old files if a target is rebuilt.

struct BuildTargetGraph {
    nodes: HashMap<BuildTargetKey, BuildTargetNode>,

    /// Keys of all nodes in the graph which have all dependencies already
    /// built.
    leaf_nodes: HashSet<BuildTargetKey>,
}

struct BuildTargetNode {
    /// Note that until the config got a target has been built we can't
    /// instantiate the target itself.
    target: Option<Arc<dyn BuildTarget>>,

    deps: BuildTargetDependencies,

    /// If true, dependencies for this node are still being added to the graph.
    /// This is mainly use to guard against cycles in expand_node.
    expanding: bool,

    usages: HashSet<BuildTargetKey>,
}

pub struct Builder {
    workspace_dir: PathBuf,
    output_dir: PathBuf,

    built_targets: HashMap<BuildTargetKey, BuildTargetOutputs>,
    rule_registry: BuildRuleRegistry,
    packages: HashMap<BuildPackageKey, BuildPackage>,
    configs: HashMap<Label, BuildConfigTarget>,
}

impl Builder {
    pub fn default() -> Result<Self> {
        let workspace_dir = PathBuf::from(common::project_dir());
        Self::new(&workspace_dir)
    }

    pub fn new(workspace_dir: &Path) -> Result<Self> {
        let mut configs = HashMap::new();
        configs.insert(
            Label::parse(NATIVE_CONFIG_LABEL)?,
            BuildConfigTarget::default_for_local_machine()?,
        );

        Ok(Self {
            workspace_dir: workspace_dir.to_path_buf(),
            output_dir: workspace_dir.join("built"),
            built_targets: HashMap::new(),
            rule_registry: BuildRuleRegistry::standard_rules()?,
            packages: HashMap::default(),
            configs,
        })
    }

    pub async fn build_target_cwd(
        &mut self,
        label: &str,
        config_label: &str,
    ) -> Result<BuildTargetOutputs> {
        let current_dir = PathBuf::from(std::env::current_dir()?);
        if !current_dir.starts_with(&self.workspace_dir) {
            return Err(err_msg("Must run the builder from inside a workspace"));
        }

        self.build_target(label, config_label, Some(&current_dir))
            .await
    }

    pub async fn build_target(
        &mut self,
        label: &str,
        config_label: &str,
        current_dir: Option<&Path>,
    ) -> Result<BuildTargetOutputs> {
        let label = self.parse_absolute_label(label, current_dir)?;
        let config_label = self.parse_absolute_label(config_label, current_dir)?;

        let target_key = BuildTargetKey {
            label,
            config_label,
        };

        // Step 1: Expand all targets to know everything we need to build.
        let mut graph = {
            let mut graph = BuildTargetGraph {
                nodes: HashMap::default(),
                leaf_nodes: HashSet::default(),
            };

            self.expand_graph_node(target_key.clone(), None, &mut graph)
                .await?;

            graph
        };

        // Step 2: Execute the graph
        while let Some(key) = graph.leaf_nodes.iter().cloned().next() {
            graph.leaf_nodes.remove(&key);
            self.execute_graph_node(key, &mut graph).await?;
        }

        let outputs = match self.built_targets.get(&target_key) {
            Some(v) => v.clone(),
            None => {
                return Err(err_msg("Failed to build entire graph for target"));
            }
        };

        Ok(outputs)
    }

    fn parse_absolute_label(&self, label: &str, current_dir: Option<&Path>) -> Result<Label> {
        let mut label = Label::parse(label)?;

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

        Ok(label)
    }

    /// TODO: The graph can be built in parallel to execution of the targets (as
    /// soon as we encounter leaf targets with no other dependencies, we can
    /// start building).
    fn expand_graph_node<'a>(
        &'a mut self,
        key: BuildTargetKey,
        parent_key: Option<BuildTargetKey>,
        graph: &'a mut BuildTargetGraph,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(self.expand_graph_node_impl(key, parent_key, graph))
    }

    async fn expand_graph_node_impl(
        &mut self,
        key: BuildTargetKey,
        parent_key: Option<BuildTargetKey>,
        graph: &mut BuildTargetGraph,
    ) -> Result<()> {
        let target_instantiated = {
            let node = {
                graph
                    .nodes
                    .entry(key.clone())
                    .or_insert_with(|| BuildTargetNode {
                        target: None,
                        deps: BuildTargetDependencies::default(),
                        expanding: false,
                        usages: HashSet::new(),
                    })
            };

            if node.expanding {
                return Err(err_msg("Recursion detected while building target graph"));
            }

            node.expanding = true;

            if let Some(parent) = parent_key {
                node.usages.insert(parent);
            }

            node.target.is_some()
        };

        if !target_instantiated {
            let mut target = None;
            let mut deps = BuildTargetDependencies::default();

            if let Some(config) = self
                .lookup_config(&key.config_label, Some(key.clone()), graph)
                .await?
            {
                let t = self.lookup_target(&key.label, &config).await?;
                deps = t.dependencies()?;
                target = Some(t);

                for dep in &deps.deps {
                    let mut dep = BuildTargetKey {
                        label: key.label.join_respecting_absolute(&dep.label)?,
                        config_label: key.label.join_respecting_absolute(&dep.config_label)?,
                    };

                    self.expand_graph_node(dep, Some(key.clone()), graph)
                        .await?;
                }
            }

            let node = graph.nodes.get_mut(&key).unwrap();
            node.target = target;
            node.deps = deps;
        }

        {
            let node = graph.nodes.get_mut(&key).unwrap();
            node.expanding = false;

            let mut all_deps_built = true;
            for dep in &node.deps.deps {
                let mut dep = BuildTargetKey {
                    label: key.label.join_respecting_absolute(&dep.label)?,
                    config_label: key.label.join_respecting_absolute(&dep.config_label)?,
                };

                if !self.built_targets.contains_key(&dep) {
                    all_deps_built = false;
                    break;
                }
            }

            // TODO: Mark the config as an explicit dependency so that we don't need to
            // check that the target is_some
            if all_deps_built && node.target.is_some() {
                graph.leaf_nodes.insert(key);
            }
        }

        Ok(())
    }

    async fn lookup_target(
        &mut self,
        label: &Label,
        context: &BuildConfigTarget,
    ) -> Result<Arc<dyn BuildTarget>> {
        assert!(label.absolute);

        let package_key = BuildPackageKey {
            name: label.directory.clone(),
            config_label: context.label.clone(),
        };

        // Directory in which the BUILD file for the required
        let package_dir = self.workspace_dir.join(&label.directory);

        let package = match self.packages.get(&package_key) {
            Some(v) => v,
            None => {
                let build_file_path = package_dir.join("BUILD");
                if !build_file_path.exists().await {
                    return Err(format_err!("Missing build file at: {:?}", build_file_path));
                }

                let build_file_data = fs::read_to_string(&build_file_path).await?;

                let package = self
                    .rule_registry
                    .evaluate_build_file(
                        package_dir.as_os_str().to_str().unwrap(),
                        &build_file_data,
                        context.config.clone(),
                    )
                    .with_context(|e| format!("While parsing {:?}: {}", build_file_path, e))?;

                self.packages.insert(package_key.clone(), package);

                self.packages.get(&package_key).unwrap()
            }
        };

        let target = match package.targets.get(label.target_name.as_str()) {
            Some(v) => v,
            None => {
                return Err(format_err!(
                    "Failed to find target named: '{}' in dir '{}'",
                    label.target_name.as_str(),
                    package_dir.to_str().unwrap()
                ));
            }
        };

        Ok(target.clone())
    }

    /// Either retreives the value of a configuration or
    async fn lookup_config(
        &mut self,
        label: &Label,
        parent_key: Option<BuildTargetKey>,
        graph: &mut BuildTargetGraph,
    ) -> Result<Option<BuildConfigTarget>> {
        assert!(label.absolute);

        if let Some(config) = self.configs.get(label) {
            return Ok(Some(config.clone()));
        }

        let target_key = BuildTargetKey {
            label: label.clone(),
            config_label: Label::parse(NATIVE_CONFIG_LABEL)?,
        };

        if let Some(outputs) = self.built_targets.get(&target_key) {
            if outputs.output_files.len() != 1 {
                return Err(err_msg("Expected configuration to only have a single file"));
            }

            let config_data =
                fs::read(&outputs.output_files.iter().next().unwrap().1.location).await?;
            let config = BuildConfig::parse(&config_data)?;

            let config_target = BuildConfigTarget::from(target_key.label.clone(), config)?;
            self.configs
                .insert(target_key.label.clone(), config_target.clone());

            return Ok(Some(config_target));
        }

        // Otherwise, the config still needs to be build, so enqueue it to be built.
        self.expand_graph_node(target_key, parent_key, graph)
            .await?;

        Ok(None)
    }

    async fn execute_graph_node(
        &mut self,
        key: BuildTargetKey,
        graph: &mut BuildTargetGraph,
    ) -> Result<()> {
        if self.built_targets.contains_key(&key) {
            return Err(err_msg("Attempting to build already built target"));
        }

        let config_hash = self
            .configs
            .get(&key.config_label)
            .unwrap()
            .config_key
            .clone();

        let mut context = BuildTargetContext {
            key: key.clone(),
            config_hash,
            workspace_dir: self.workspace_dir.clone(),
            package_dir: self.workspace_dir.join(&key.label.directory),
            inputs: HashMap::new(),
        };

        let node = graph
            .nodes
            .get(&key)
            .ok_or_else(|| err_msg("Missing node for executing target"))?;

        let target = node
            .target
            .as_ref()
            .ok_or_else(|| err_msg("Missing target for executing target"))?;

        for dep in &node.deps.deps {
            let absolute_dep = BuildTargetKey {
                label: key.label.join_respecting_absolute(&dep.label)?,
                config_label: key.label.join_respecting_absolute(&dep.config_label)?,
            };

            let outputs = self
                .built_targets
                .get(&absolute_dep)
                .ok_or_else(|| err_msg("Building target before dependencies complete"))?;

            // NOTE: Using the original key provided by the Target instance.
            context.inputs.insert(dep.clone(), outputs.clone());
        }

        let outputs = target.build(&context).await?;
        self.built_targets.insert(key, outputs);

        let usages = node.usages.clone();
        for usage in usages {
            println!("EXPAND PARENT: {:?}", usage);
            self.expand_graph_node(usage, None, graph).await?;
        }

        Ok(())
    }
}

// #[derive(Debug, Clone)]
// pub struct OutputFile {
//     /// NOTE: Every single OutputFile must have a distinct mount_path. This
// must     /// also be unique across different rules.
//     pub mount_path: String,

//     pub source_path: PathBuf,
// }
