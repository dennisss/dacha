use common::async_std::fs;
use common::async_std::path::Path;
use common::errors::*;

pub async fn create_or_update_symlink<P: AsRef<Path>, P2: AsRef<Path>>(
    original: P,
    link_path: P2,
) -> Result<()> {
    let original = original.as_ref();
    let link_path = link_path.as_ref();

    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    if let Ok(_) = link_path.symlink_metadata().await {
        fs::remove_file(&link_path).await?;
    }

    std::os::unix::fs::symlink(original, link_path)?;

    Ok(())
}
