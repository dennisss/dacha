use common::errors::*;
use mocap_proto::mocap::*;
use file::{LocalPath, LocalPathBuf};
use protobuf::Message;
use crypto::hasher::Hasher;

const DEFAULT_CONFIG_HASH_FILE: &'static str = "config_default_hash";
const BASE_CONFIG_FILE: &'static str = "config_base.txtpb";

pub async fn read_base_config(data_dir: &LocalPath) -> Result<MocapManagerConfig> {
    let default_config_data = file::read_asset_to_str("pkg/vision/mocap/config/manager.txtpb").await?; 
    let mut default_config = MocapManagerConfig::default();
    protobuf::text::parse_text_proto(
        &default_config_data,
        &mut default_config
    )?;

    let default_config_hash = {
        let data = default_config.serialize()?;
        let mut hasher = crypto::md5::MD5Hasher::default();
        hasher.update(&data);
        hasher.finish()
    };

    let base_config = read_base_config_from_fs(&data_dir, &default_config_hash).await?;

    let base_config = match base_config {
        Some(v) => v,
        None => {
            println!("Newly installing base config...");
            let hash_file_path = data_dir.join(DEFAULT_CONFIG_HASH_FILE);
            let base_path = data_dir.join(BASE_CONFIG_FILE);

            file::write(&base_path, default_config_data.as_bytes()).await?;
            file::write(&hash_file_path, &default_config_hash).await?;

            default_config.clone()
        }
    };

    Ok(base_config)
}

async fn read_base_config_from_fs(data_dir: &LocalPath, default_config_hash: &[u8]) -> Result<Option<MocapManagerConfig>> {
    let hash_file_path = data_dir.join(DEFAULT_CONFIG_HASH_FILE);
    if !file::exists(&hash_file_path).await? {
        return Ok(None);
    }

    let old_hash = file::read(&hash_file_path).await?;
    if &old_hash != default_config_hash {
        return Ok(None);
    }

    let base_path = data_dir.join(BASE_CONFIG_FILE);
    if !file::exists(&base_path).await? {
        return Ok(None);
    }

    let data = file::read(&base_path).await?;

    let data_str = match std::str::from_utf8(&data) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Base config is invalid UTF8. Discarding...");
            return Ok(None);
        }
    };

    let mut config = MocapManagerConfig::default();
    let res = protobuf::text::parse_text_proto(
        data_str,
        &mut config
    );

    match res {
        Ok(()) => {
            Ok(Some(config))
        },
        Err(e) => {
            eprintln!("Base config is invalid: {}. Discarding...", e);
            Ok(None)
        }
    }
}