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

        let deb = match self.component.as_str() {
            "camera" => "dist/pkg/vision/mocap/mocap-camera.deb",
            "supervisor" => "dist/pkg/vision/mocap/mocap-supervisor.deb",
            "kernel" => "dist/pkg/rpi/linux-kernel-dacha-rpi-arm64.deb",
            "ar0234" => "dist/pkg/rpi/ar0234-driver-rpi-arm64.deb",
            _ => return Err(err_msg("Unknown component"))
        };

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

        let (mut req_stream, res_stream) = stub.Update(&rpc::ClientRequestContext::default()).await;
        let mut client = UpdateClient {
            req_stream,
            res_stream
        };

        {
            let mut req = UpdateRequest::default();
            req.start_update_mut();
            client.send(&req).await?;
        }

        let data = file::read(deb_path).await?;

        for chunk in data.chunks(8192) {
            let mut req = UpdateRequest::default();
            req.set_payload_chunk(chunk);
            client.send(&req).await?;
        }

        {
            let mut req = UpdateRequest::default();
            req.set_install_deb(true);
            client.send(&req).await?;
        }

        println!("Commiting...");


        {
            let mut req = UpdateRequest::default();
            req.commit_update_mut();
            client.send(&req).await?;
        }

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

struct UpdateClient {
    req_stream: rpc::ClientStreamingRequest<UpdateRequest>,
    res_stream: rpc::ClientStreamingResponse<UpdateResponse>,
}

impl UpdateClient {

    pub async fn send(&mut self, req: &UpdateRequest) -> Result<()> {

        if !self.req_stream.send(req).await {
            self.req_stream.close().await;
        }

        if let Some(res) = self.res_stream.recv().await {
            // println!("Got it: {:?}", res);
        } else {
            self.res_stream.finish().await?;

            return Err(err_msg("Stream ended without an error"));
        }

        Ok(())
    }

}
