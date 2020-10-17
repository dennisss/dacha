pub mod allocate_soft;
mod dir_lock;

pub use self::dir_lock::DirLock;

use std::path::Path;


/// Based on the example: https://doc.rust-lang.org/std/fs/fn.read_dir.html#examples
pub fn recursively_list_dir(dir: &Path, callback: &mut dyn FnMut(&std::fs::DirEntry)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                recursively_list_dir(&path, callback)?;
            } else {
                callback(&entry);
            }
        }
    }
    Ok(())
}