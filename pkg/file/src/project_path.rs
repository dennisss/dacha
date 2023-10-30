use crate::LocalPathBuf;

/// Gets the root directory of this project (the directory that contains the
/// 'pkg' and '.git' directory).
pub fn project_dir() -> LocalPathBuf {
    let mut dir = crate::current_dir().unwrap();

    // TOOD: Instead base this on finding a WORKSPACE file or environment variable?

    // Special case which running in the 'cross' docker container.
    if dir.starts_with("/project") {
        return "/project".into();
    }

    loop {
        match dir.file_name() {
            Some(name) => {
                if name == "dacha" {
                    break;
                }

                dir.pop();
            }
            None => {
                panic!(
                    "Failed to find project dir in: {:?}",
                    crate::current_dir().unwrap()
                );
            }
        }
    }

    dir
}

#[macro_export]
macro_rules! project_path {
    // TODO: Assert that relpath is relative and not absolute.
    ($relpath:expr) => {
        $crate::project_dir().join($relpath)
    };
}
