use std::collections::HashMap;
use std::sync::Arc;

use crate::label::*;
use crate::target::*;

#[derive(Default)]
pub struct BuildPackage {
    pub targets: HashMap<String, Arc<dyn BuildTarget>>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BuildPackageKey {
    pub name: String,
    pub config_label: Label,
}
