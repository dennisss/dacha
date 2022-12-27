use common::errors::*;
use file::LocalPath;

pub async fn create_or_update_symlink<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(
    original: P,
    link_path: P2,
) -> Result<()> {
    let original = original.as_ref();
    let link_path = link_path.as_ref();

    if let Some(parent) = link_path.parent() {
        file::create_dir_all(parent).await?;
    }

    // TODO: Check this.
    if let Ok(_) = file::symlink_metadata(&link_path).await {
        file::remove_file(&link_path).await?;
    }

    std::os::unix::fs::symlink(original, link_path)?;

    Ok(())
}
