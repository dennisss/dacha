use common::errors::*;

use crate::{ControlFile, ControlFileStanza};

pub struct ReleaseFile {
    stanza: ControlFileStanza,
}

impl ReleaseFile {
    pub fn try_from(mut file: ControlFile) -> Result<Self> {
        if file.stanzas.len() != 1 {
            return Err(err_msg("Expected Release file to have one stanza"));
        }

        Ok(Self {
            stanza: file.stanzas.pop().unwrap(),
        })
    }

    pub fn architectures(&self) -> Vec<String> {
        let value = match self.stanza.fields.get("Architectures") {
            Some(v) => v,
            None => return vec![],
        };

        value.split_whitespace().map(|v| v.to_string()).collect()
    }

    pub fn components(&self) -> Vec<String> {
        let value = match self.stanza.fields.get("Components") {
            Some(v) => v,
            None => return vec![],
        };

        value.split_whitespace().map(|v| v.to_string()).collect()
    }

    pub fn sha256(&self) -> Result<Vec<ReleaseFileEntry>> {
        let value = match self.stanza.fields.get("SHA256") {
            Some(v) => v,
            None => return Ok(vec![]),
        };

        let mut out = vec![];
        for line in value.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let fields = line.split_whitespace().collect::<Vec<&str>>();
            if fields.len() != 3 {
                return Err(err_msg("Expected 3 fields in each SHA256 entry"));
            }

            let hash = fields[0].to_string();
            let size = fields[1].parse()?;
            let path = fields[2].to_string();

            out.push(ReleaseFileEntry { hash, size, path });
        }

        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct ReleaseFileEntry {
    pub hash: String,
    pub size: usize,
    pub path: String,
}
