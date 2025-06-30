use base_error::*;
use file::{LocalPath, FileFilter, FileFilterDecision};

use crate::{GitIgnore, GitSubmodules};

// TODO: Support gitignores that are not in the root directory.
pub struct GitFileFilter {
    ignore: GitIgnore,
    submodules: GitSubmodules
}

impl GitFileFilter {
    pub async fn create() -> Result<Self> {
        let ignore = GitIgnore::read().await?;
        let submodules = GitSubmodules::read().await?;

        assert!(ignore.should_ignore(LocalPath::new("/home/dennis/workspace/dacha/x/node_modules"), true));
        assert!(ignore.should_ignore(LocalPath::new("/home/dennis/workspace/dacha/node_modules"), true));

        Ok(Self {
            ignore, submodules
        })
    }
}

impl FileFilter for GitFileFilter {
    fn filter_file(&self, path: &LocalPath, is_dir: bool) -> FileFilterDecision {
        let skip = FileFilterDecision { emit: false, traverse: false };
        
        if self.ignore.should_ignore(path, is_dir) {
            return skip;
        }

        if is_dir {
            for submodule in &self.submodules.submodules {
                if path == &submodule.path {
                    return skip;
                }
            }
        }

        FileFilterDecision { emit: !is_dir, traverse: true }
    }
}
