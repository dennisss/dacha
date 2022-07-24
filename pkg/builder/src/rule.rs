use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::errors::*;
use common::factory::Factory;
use common::failure::ResultExt;
use google::proto::any::Any;
use protobuf::{Message, MessageReflection, StaticMessage};

use crate::package::*;
use crate::proto::config::*;
use crate::proto::rule::*;
use crate::target::BuildTarget;

pub trait BuildRule {
    type Attributes;
    type Target;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target>;
}

#[derive(Default)]
pub struct BuildRuleRegistry {
    functions: HashMap<String, BuildRuleFunction>,
}

type BuildRuleFunction =
    fn(&mut skylark::FunctionCallContext, &BuildConfig) -> Result<Arc<dyn BuildTarget>>;

impl BuildRuleRegistry {
    pub fn standard_rules() -> Result<Self> {
        let mut inst = BuildRuleRegistry::default();
        inst.register::<crate::rules::filegroup::FileGroup>("filegroup")?;
        inst.register::<crate::rules::rust_binary::RustBinary>("rust_binary")?;
        inst.register::<crate::rules::webpack::Webpack>("webpack")?;
        inst.register::<crate::rules::bundle::Bundle>("bundle")?;
        inst.register::<crate::rules::local_binary::LocalBinary>("local_binary")?;
        inst.register::<crate::rules::proto_data::ProtoData>("proto_data")?;

        Ok(inst)
    }

    /// Registers a new rule which can be called in Skylark code using
    /// 'rule_name()'.
    pub fn register<R: BuildRule>(&mut self, rule_name: &str) -> Result<()>
    where
        R::Attributes: StaticMessage,
        R::Target: BuildTarget,
    {
        if self
            .functions
            .insert(rule_name.to_string(), Self::rule_function::<R>)
            .is_some()
        {
            return Err(format_err!(
                "Duplicate rule definition named: {}",
                rule_name
            ));
        }

        Ok(())
    }

    fn rule_function<R: BuildRule>(
        ctx: &mut skylark::FunctionCallContext,
        config: &BuildConfig,
    ) -> Result<Arc<dyn BuildTarget>>
    where
        R::Attributes: StaticMessage,
        R::Target: BuildTarget,
    {
        // TODO: Validate that this is only being called from a BUILD file and not from
        // a loaded utility function (unless in a macro)

        let mut attributes = R::Attributes::default();

        for defaults in config.rule_defaults() {
            if defaults.type_url() == attributes.type_url() {
                attributes = defaults
                    .unpack()?
                    .ok_or_else(|| err_msg("Failed to unpack rule defaults"))?;
                break;
            }
        }

        ctx.args_iter()?.to_proto(&mut attributes)?;

        let target = R::evaluate(attributes, config)?;

        Ok(Arc::new(target))
    }

    pub fn evaluate_build_file(
        &self,
        source_path: &str,
        source: &str,
        config: Arc<BuildConfig>,
    ) -> Result<BuildPackage> {
        let mut package = Arc::new(Mutex::new(BuildPackage::default()));

        let mut universe = skylark::Universe::new()?;
        for (rule_name, func) in self.functions.iter() {
            let rule_name = rule_name.clone();
            let func = *func;

            let package = package.clone();
            let config = config.clone();
            universe.bind_function(rule_name.as_str(), move |mut ctx| {
                let target = func(&mut ctx, &config)?;
                let target_name = target.name().to_string();

                let mut package = package.lock().unwrap();
                if package
                    .targets
                    .insert(target_name.clone(), target)
                    .is_some()
                {
                    return Err(format_err!(
                        "Duplicate target in package named '{}'",
                        target_name
                    ));
                }

                ctx.pool().insert(skylark::NoneValue::new())
            })?;
        }

        // TODO: Re-use the same environment across runs so that we can support caching
        // of any loading files.
        {
            let env = skylark::Environment::new(universe)?;
            env.evaluate_file(source_path, source)
                .with_context(|e| format_err!("While evaluating skylark: {}", e))?;
        }

        let package = Arc::try_unwrap(package)
            .map_err(|_| err_msg("Remaining references to package data"))?
            .into_inner()?;

        Ok(package)
    }
}
