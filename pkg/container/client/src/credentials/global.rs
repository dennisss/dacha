use std::sync::Arc;
use std::sync::Once;

use common::errors::*;
use crypto::tls::FileCredentialsLoader;
use executor::lock;
use executor::sync::AsyncMutex;
use file::LocalPath;

use crate::env::CREDENTIALS_DIR_ENV_VAR;

// TODO: Also store a result if it failed.
static ENV_CREDENTIALS: AsyncMutex<Option<Arc<FileCredentialsLoader>>> = AsyncMutex::new(None);

/// TODO: Eventually make private to this crate.
pub async fn get_cluster_credentials() -> Result<Arc<FileCredentialsLoader>> {
    let guard = ENV_CREDENTIALS.lock().await?.read_exclusive();
    if let Some(v) = guard.as_ref() {
        return Ok(v.clone());
    }

    let dir = std::env::var(CREDENTIALS_DIR_ENV_VAR).map_err(|_| {
        format_err!(
            "Expected the {} environment variable to be set",
            CREDENTIALS_DIR_ENV_VAR,
        )
    })?;

    let v = Arc::new(FileCredentialsLoader::create(LocalPath::new(&dir)).await?);

    lock!(state <= guard.upgrade(), {
        *state = Some(v.clone());
    });

    Ok(v)
}
