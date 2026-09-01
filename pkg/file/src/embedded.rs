use alloc::string::String;

use std::sync::{Mutex, LazyLock};
use std::collections::HashMap;
use std::borrow::Cow;

use common::hash::FastHasherBuilder;
use common::errors::*;

use crate::error::*;
use crate::maybe_project_dir;

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::default());

#[derive(Default)]
struct State {
    assets: HashMap<&'static str, &'static [u8], FastHasherBuilder>
}

pub fn register_asset(path: &'static str, data: &'static [u8]) -> Result<()> {
    let mut state = STATE.lock().unwrap();
    state.assets.insert(path, data);
    Ok(())
}

// This will fallback to reading from fs if missing in memory.
pub async fn read_asset(path: &str) -> Result<Cow<'static, [u8]>> {
    if let Some(dir) = maybe_project_dir() {
        // NOTE: This is not cached to enable live reloads.
        let data = crate::read(dir.join(path)).await?;
        // TODO: Warn if not in the 'assets' (this will eventually need to be start enough to
        // deal with directories with variable file sets for live reloading)
        return Ok(Cow::Owned(data));
    }

    let state = STATE.lock().unwrap();
    let data = state.assets.get(path).map(|s| *s)
        .ok_or_else(|| Error::from(FileError::new(
            FileErrorKind::NotFound,
            &format!("Asset file does not exist: {}", path)
        )))?;
    Ok(Cow::Borrowed(data))
}

pub async fn read_asset_to_str(path: &str) -> Result<Cow<'static, str>> {
    let data = read_asset(path).await?;
    Ok(match data {
        Cow::Owned(s) => Cow::Owned(String::from_utf8(s)?),
        Cow::Borrowed(s) => Cow::Borrowed(std::str::from_utf8(s)?)
    })
}