mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/index.rs"));
}

use std::collections::HashMap;

use base_error::*;

use common::check_zero_padding;
pub use proto::*;

pub async fn read_index() -> Result<Index> {
    let data = file::read(file::project_path!(".git/index")).await?;

    let (v, _) = Index::parse(&data)?;

    if &v.signature[..] != b"DIRC" {
        return Err(err_msg("Invalid index signature"))?;
    }

    // NOTE: In v4, names are prefix compressed.
    if v.version != 2 {
        return Err(err_msg("Unsupported git index version"));
    }

    for entry in &v.entries {
        check_zero_padding(&entry.padding)?;
    }

    Ok(v)
}
