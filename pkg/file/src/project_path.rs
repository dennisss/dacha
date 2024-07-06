use crate::LocalPathBuf;

/// Gets the root directory of this project (the directory that contains the
/// 'pkg' and '.git' directory).
pub fn project_dir() -> LocalPathBuf {
    let mut dir = crate::current_dir().unwrap();

    loop {
        if let Ok(true) = crate::exists_sync(dir.join("WORKSPACE")) {
            return dir;
        }

        dir.pop();
    }

    panic!(
        "Failed to find project dir in: {:?}",
        crate::current_dir().unwrap()
    );
}

#[macro_export]
macro_rules! project_path {
    // TODO: Assert that relpath is relative and not absolute.
    ($relpath:expr) => {
        $crate::project_dir().join($relpath)
    };
}
