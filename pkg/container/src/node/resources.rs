use std::collections::HashSet;

/// Collection fo
#[derive(Default)]
pub(super) struct ResourceSet {
    /// Set of blob ids needed.
    pub blobs: HashSet<String>,
}

impl ResourceSet {
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }
}
