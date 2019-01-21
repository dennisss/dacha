use super::protos::*;
use std::collections::HashMap;

/// This is the format of the metadata file being persisted to disk
#[derive(Serialize, Deserialize)]
pub struct ServerMetadata {
	pub cluster_id: u64,

	pub id: ServerId,

	pub meta: Metadata
}

#[derive(Serialize)]
pub struct ServerMetadataRef<'a> {
	pub cluster_id: u64,
	pub id: ServerId,
	pub meta: &'a Metadata
}

/// This is the format of the file on disk for the snapshot of the configuration
#[derive(Serialize, Deserialize)]
pub struct ServerConfigurationSnapshot {

	pub config: ConfigurationSnapshot,

	pub routes: HashMap<ServerId, String>

}

#[derive(Serialize)]
pub struct ServerConfigurationSnapshotRef<'a> {
	pub config: ConfigurationSnapshotRef<'a>,
	pub routes: &'a HashMap<ServerId, String>
}

impl Default for ServerConfigurationSnapshot {
	fn default() -> Self {
		ServerConfigurationSnapshot {
			config: ConfigurationSnapshot::default(),
			routes: HashMap::new()
		}
	}
}
