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

use crate::components::*;

#[derive(Args)]
pub struct UpdateCommand {
    #[arg(positional)]
    component: String
}

impl UpdateCommand {
    pub async fn run(self) -> Result<()> {

        let components = Component::all();

        let component = components.iter().find(|c| c.name == self.component)
            .ok_or_else(|| format_err!("Unknown component: {}", self.component))?;
        
        let artifact_path = project_dir().join(&component.artifact);

        let mut resolver = CameraResolver::create().await?;
        let cams = resolver.resolve().await?;

        if cams.is_empty() {
            return Err(err_msg("No cameras found to update"));
        }

        println!("Found {} cameras", cams.len());

        for (camera_id, endpoint) in cams {
            println!("- Updating: {}", endpoint);

            match component.updater {
                ComponentUpdater::DebUpdate => {
                    Self::perform_deb_update(&resolver, &endpoint, &artifact_path).await?;
                }
                ComponentUpdater::MCUFlash => {
                    Self::perform_mcu_flash(&resolver, &endpoint, &artifact_path).await?;
                }
            }
        }

        Ok(())

    }

    async fn perform_deb_update(resolver: &CameraResolver, endpoint: &str, deb_path: &LocalPath) -> Result<()> {
        let stub = resolver.connect_to_supervisor(&endpoint).await?;

        let mut client = UpdateClient::create(&stub).await?;

        client.start_update().await?;

        let data = file::read(deb_path).await?;
        client.send_payload(&data).await?;

        {
            let mut req = UpdateRequest::default();
            req.set_install_deb(true);
            client.send(&req).await?;
        }

        println!("Commiting...");

        client.commit_update().await?;

        println!("=> Done!");

        Ok(())
    }

    async fn perform_mcu_flash(resolver: &CameraResolver, endpoint: &str, firmware_path: &LocalPath) -> Result<()> {

        let firmware = file::read(firmware_path).await?;

        let stub = resolver.connect(endpoint).await?.camera_stub;

        let mut req = FlashMCURequest::default();
        req.set_firmware(firmware);

        let ctx = rpc::ClientRequestContext::default();

        let res = stub.FlashMCU(&ctx, &req).await.result?;

        println!("=> Done!");

        Ok(())

    }
}


