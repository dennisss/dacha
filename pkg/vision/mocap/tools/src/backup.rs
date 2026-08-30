use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use file::{LocalPath, LocalPathBuf};
use file::{project_path, project_dir};
use mocap_manager::networking::*;
use mocap_proto::mocap::*;
use cluster_client::id::entity_id_to_string;
use protobuf::Message;

use crate::components::*;

#[derive(Args)]
pub struct BackupCommand {

}

impl BackupCommand {

    pub async fn run(self) -> Result<()> {
        
        let output_dir = project_path!("data/mocap_backups");

        let mut resolver = CameraResolver::create().await?;
        let cams = resolver.resolve().await?;

        if cams.is_empty() {
            return Err(err_msg("No cameras found to backup"));
        }

        println!("Found {} cameras", cams.len());

        for (camera_id, endpoint) in cams {
            let camera_id_str = entity_id_to_string(camera_id).unwrap();

            println!("- Backing up: {} : {}", camera_id, endpoint);

            let stub = resolver.connect_to_supervisor(&endpoint).await?;

            let status = stub.Status(&rpc::ClientRequestContext::default(), &SupervisorStatusRequest::default()).await.result?;

            let output_dir = output_dir.join(camera_id_str);
            file::create_dir_all(&output_dir).await?;

            println!("Hardware Config: {:?}", status.hardware_config());

            file::write(output_dir.join("hardware_config.pb"), &status.hardware_config().serialize()?);
        }

        Ok(())

    }
}
