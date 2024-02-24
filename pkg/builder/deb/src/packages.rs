use std::collections::HashMap;

use common::errors::*;

use crate::{ControlFile, ControlFileStanza};

pub struct PackagesFile {
    values: HashMap<String, Vec<Package>>,
}

impl PackagesFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let file = ControlFile::parse(std::str::from_utf8(data)?)?;

        let mut values = HashMap::new();

        for stanza in file.stanzas {
            let pkg = Package { stanza };

            let name = pkg.name()?;
            let version = pkg.version()?;

            // NOTE: There may be multiple packages at different versions or architectures.
            values
                .entry(name.to_string())
                .or_insert_with(|| vec![])
                .push(pkg);

            // let key = (name.to_string(), version.to_string());

            // TODO: Use a key of (name, arch, version, )
            // ^ There is also a special "all" which shouldn't conflict with it.
            // Repositories are allowed to contain multiple packages with the
            // same name at different versions.
            // if values.contains_key(&key) {
            //     return Err(format_err!("Duplicate package named: {}", name));
            // }

            // values.insert(key, pkg);
        }

        Ok(Self { values })
    }

    pub fn packages(&self) -> impl Iterator<Item = &Package> {
        self.values.values().map(|v| v.iter()).flatten()
    }
}

pub struct Package {
    stanza: ControlFileStanza,
}

impl Package {
    fn get_mandatory_field(&self, name: &str) -> Result<&str> {
        self.stanza
            .fields
            .get(name)
            .map(|s| s.as_str())
            .ok_or_else(|| format_err!("Package missing mandatory file: {}", name))
    }

    pub fn name(&self) -> Result<&str> {
        self.get_mandatory_field("Package")
    }

    pub fn filename(&self) -> Result<&str> {
        self.get_mandatory_field("Filename")
    }

    pub fn version(&self) -> Result<&str> {
        self.get_mandatory_field("Version")
    }

    pub fn size(&self) -> Result<usize> {
        let v = self.get_mandatory_field("Size")?;
        Ok(v.parse()?)
    }
}
