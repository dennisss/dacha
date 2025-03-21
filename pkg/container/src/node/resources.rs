use std::collections::HashSet;

/// Collection fo
#[derive(Default, Debug)]
pub(super) struct ResourceSet {
    /// Set of blob ids needed.
    pub blobs: HashSet<String>,

    /// If true, credentials haven't been generated for the worker.
    pub credentials: bool,
}

impl ResourceSet {
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty() && !self.credentials
    }
}
