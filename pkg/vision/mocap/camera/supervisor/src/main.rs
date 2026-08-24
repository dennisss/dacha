#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

mod run;
mod update;
mod dns;

use std::time::Duration;
use std::sync::Arc;

use common::errors::*;
use executor_multitask::RootResource;
use cluster_client::id::entity_id_to_string;
use mocap_proto::mocap::*;
use file::LocalPath;
use protobuf::StaticMessage;

use crate::dns::*;
use crate::update::*;


/*
TODO: set IP_MULTICAST_IF for outgoing multicast packets
TODO: Add interface to ADD_MEMBERSHIP for incoming packets.

*/


#[derive(Default)]
struct SupervisorInst {
    updater: Updater,
    hardware_config: CameraHardwareConfig
}

#[async_trait]
impl SupervisorService for SupervisorInst {

    async fn Status(
        &self,
        request: rpc::ServerRequest<SupervisorStatusRequest>,
        response: &mut rpc::ServerResponse<SupervisorStatusResponse>
    ) -> Result<()> {
        response.set_hardware_config(self.hardware_config.clone());
        Ok(())
    }

    async fn Run(
        &self,
        request: rpc::ServerRequest<SupervisorRunRequest>,
        response: &mut rpc::ServerResponse<SupervisorRunResponse>
    ) -> Result<()> {
        response.value = crate::run::run_command(&request)?;
        Ok(())
    }

    async fn ReadFile(
        &self,
        request: rpc::ServerRequest<SupervisorReadFileRequest>,
        response: &mut rpc::ServerStreamResponse<SupervisorReadFileResponse>
    ) -> Result<()> {
        let data = file::read(request.path()).await?;

        let mut res = SupervisorReadFileResponse::default();
        res.set_data(data);

        response.send(res).await?;

        Ok(())
    }    

    async fn Update(
        &self,
        req_stream: rpc::ServerStreamRequest<UpdateRequest>,
        res_stream: &mut rpc::ServerStreamResponse<UpdateResponse>
    ) -> Result<()> {
        self.updater.update(req_stream, res_stream).await
    }
}


// TODO: Probably just limit this to one thread.
#[executor_main]
async fn main() -> Result<()> {

    let hardware_config = {
        let data = file::read("/boot/firmware/camera_hardware.pb").await?;
        CameraHardwareConfig::parse(&data)?
    };

    let camera_id = {
        if hardware_config.camera_id() == 0 {
            return Err(err_msg("Hardware config is missing a valid camera id"));
        }
        entity_id_to_string(hardware_config.camera_id()).unwrap()
    };

    println!("Camera Id: {}", camera_id);

    // Normalize initial state which may be wrong if the supervisor restarted mid update.
    if let Err(e) = Updater::toggle_writeable_fs(false) {
        eprintln!("Failed to mark FS as read only: {}", e);
    }

    // TODO: Call gethostname/sethostname, set /etc/hostname, etc.

    let service = RootResource::new();

    // TODO: The join_multicast_v4 in here will currently fail with '[ENODEV] No such device' if we haven't gotten an IP address yet.
    let dns_server = net::dns::Server::create_multicast_insecure(Arc::new(DNSServerHandler::new(camera_id.clone()))).await?;
    service.register_dependency(Arc::new(dns_server)).await;


    let mut rpc_server = rpc::Http2Server::new(Some(81));
    let inst = Arc::new(SupervisorInst {
        updater: Default::default(),
        hardware_config
    });
    rpc_server.add_service(inst.clone().into_service())?;
    service.register_dependency(rpc_server.start()).await;

    println!("Running...");
    service.wait().await
}