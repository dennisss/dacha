use alloc::string::String;

use common::errors::*;

use crate::language::*;
use crate::linux::Device;

impl Device {
    pub async fn read_local_language(&self) -> Result<Option<Language>> {
        let mut languages = self.read_languages().await?;

        // TODO: Support adapting this to the local system's settings.
        languages.sort_by_key(|lang| {
            if lang.primary_language() == PrimaryLanguage::English {
                if lang.sub_language() == SubLanguage::US {
                    return 1;
                }
                if lang.sub_language() == SubLanguage::UK {
                    return 2;
                }

                return 3;
            }

            4
        });

        Ok(languages.get(0).cloned())
    }

    pub async fn read_local_string(&self, index: u8) -> Result<String> {
        // NOTE: This is also checked in read_string but we check it early as devices
        // without strings don't export any languages.
        if index == 0 {
            return Err(err_msg("Attempting to read reserved string index 0"));
        }

        let language = self
            .read_local_language()
            .await?
            .ok_or_else(|| err_msg("Device has not defined any string languages"))?;
        self.read_string(index, language).await
    }
}
