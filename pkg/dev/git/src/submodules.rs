use base_error::*;
use file::{GlobIterator, LocalPath, LocalPathBuf};

pub struct GitSubmodules {
    pub submodules: Vec<GitSubmodule>
}

pub struct GitSubmodule {
    pub path: LocalPathBuf,
}

impl GitSubmodules {
    // TODO: Implement all features in this.
    pub async fn read() -> Result<Self> {
        let mut submodules = vec![];

        let base_dir = file::project_dir();

        // TODO: Skip if this file is missing.
        let data = file::read_to_string(file::project_path!(".gitmodules")).await?;

        for line in data.lines() {
            let mut line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(path) = line.strip_prefix("path = ") {
                submodules.push(GitSubmodule {
                    path: base_dir.join(path)
                });
            }
        }

        Ok(Self {
            submodules
        })
    }
}