use common::errors::*;

/// NOTE: In order to hash or compare this, the user should be sure to make the
/// label absolute first.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub absolute: bool,
    pub directory: String,
    pub target_name: String,
}

impl Label {
    pub fn parse(mut label: &str) -> Result<Self> {
        let absolute = if let Some(v) = label.strip_prefix("//") {
            label = v;
            true
        } else {
            false
        };

        let (dir, name) = label
            .split_once(':')
            .ok_or_else(|| err_msg("Expected a : in the target path"))?;

        if dir.starts_with("/") {
            return Err(err_msg("Invalid directory in target path"));
        }

        Ok(Self {
            absolute,
            directory: dir.to_string(),
            target_name: name.to_string(),
        })
    }
}
