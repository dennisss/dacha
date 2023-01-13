use std::collections::HashMap;
use std::sync::Arc;

use crate::table::bloom::BloomFilterPolicy;

pub trait FilterPolicy: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn create(&self, keys: Vec<&[u8]>, out: &mut Vec<u8>);
    fn key_may_match(&self, key: &[u8], filter: &[u8]) -> bool;
}

pub struct FilterPolicyRegistry {
    policies: HashMap<String, Arc<dyn FilterPolicy>>,
}

impl Default for FilterPolicyRegistry {
    fn default() -> Self {
        let mut policies: HashMap<String, Arc<dyn FilterPolicy>> = HashMap::new();

        let p = BloomFilterPolicy::default();
        policies.insert(p.name().to_string(), Arc::new(p));

        Self { policies }
    }
}

impl FilterPolicyRegistry {
    pub fn get(&self, name: &str) -> Option<Arc<dyn FilterPolicy>> {
        self.policies.get(name).map(|v| v.clone())
    }

    /// Creates a new filter registry derived from the current one where each
    /// policy is transformed by the given function.
    pub fn wrap(&self, wrapper: fn(Arc<dyn FilterPolicy>) -> Arc<dyn FilterPolicy>) -> Self {
        let mut policies = self.policies.clone();
        for (_, policy) in policies.iter_mut() {
            *policy = wrapper(policy.clone());
        }

        Self { policies }
    }
}
