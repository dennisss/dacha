use std::sync::LazyLock;

use common::errors::*;

use crate::LocalPathBuf;


static PROJECT_DIR: LazyLock<ProjectDirState> = LazyLock::new(|| {
    let current_dir = crate::current_dir().unwrap();
    let mut project_dir = None;

    let mut dir = current_dir.clone();
    loop {
        if let Ok(true) = crate::exists_sync(dir.join("WORKSPACE")) {
            project_dir = Some(dir);
            break;
        }

        if !dir.pop() {
            break;
        }
    }

    ProjectDirState {
        current_dir,
        project_dir
    }
});

struct ProjectDirState {
    current_dir: LocalPathBuf,
    project_dir: Option<LocalPathBuf>
}

/// Gets the root directory of this project (the directory that contains the
/// 'pkg' and '.git' directory).
pub fn project_dir() -> LocalPathBuf {
    try_project_dir().unwrap()
}

pub fn try_project_dir() -> Result<LocalPathBuf> {
    match &PROJECT_DIR.project_dir {
        Some(v) => return Ok(v.clone()),
        None => {
            Err(format_err!(
                "Failed to find project dir in: {}",
                PROJECT_DIR.current_dir.display()
            ))
        }
    }
}

pub fn maybe_project_dir() -> Option<LocalPathBuf> {
    PROJECT_DIR.project_dir.clone()
}


#[macro_export]
macro_rules! project_path {
    // TODO: Assert that relpath is relative and not absolute.
    ($relpath:expr) => {
        $crate::project_dir().join($relpath)
    };
}
