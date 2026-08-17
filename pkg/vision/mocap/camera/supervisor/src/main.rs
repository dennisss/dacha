#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

mod run;

use std::time::Duration;
use std::sync::Arc;
use std::os::unix::process::CommandExt;

use common::errors::*;
use executor_multitask::RootResource;
use cluster_client::id::{entity_id_to_string, normalize_entity_id};
use mocap_proto::mocap::*;
use file::LocalPath;


/*
TODO: set IP_MULTICAST_IF for outgoing multicast packets
TODO: Add interface to ADD_MEMBERSHIP for incoming packets.

*/

struct DNSServerHandler {
    camera_id: String,
}

#[async_trait]
impl net::dns::ServerHandler for DNSServerHandler {
    fn handle_connection(&self, peer_addr: &net::ip::SocketAddr) -> bool {
        true
    }

    async fn handle_question(
        &self,
        question: &net::dns::Question<'_>,
        response: &mut net::dns::ReplyBuilder
    ) -> Result<()> {
        let question_name = question.name().to_string();

        let our_host_name = format!("mocap-camera-{}", self.camera_id);
        let our_dns_name = format!("{}.local.", our_host_name);        
        let our_service_name = format!("camera-{}._mocap._tcp.local.", self.camera_id);
        
        if question_name == "_mocap._tcp.local." &&
            question.class() == net::dns::Class::IN &&
            question.typ() == net::dns::RecordType::PTR
        {
            response.add_answer(
                question.name(),
                net::dns::RecordType::PTR,
                net::dns::Class::IN,
                5 * 60,
                &net::dns::ResourceRecordData::Pointer(our_service_name.as_str().try_into()?)
            );
        }

        if question_name == our_service_name &&
            question.class() == net::dns::Class::IN &&
            question.typ() == net::dns::RecordType::SRV
        {
            response.add_answer(
                question.name(),
                net::dns::RecordType::SRV,
                net::dns::Class::IN,
                5 * 60,
                &net::dns::ResourceRecordData::Service(net::dns::SRVRecordData {
                    header: net::dns::SRVDataHeader {
                        priority: 0,
                        weight: 0,
                        port: 80
                    },
                    target: our_dns_name.as_str().try_into()?
                })
            );
        }

        if question_name == our_dns_name &&
            question.class() == net::dns::Class::IN &&
            question.typ() == net::dns::RecordType::A
        {
            let ip = net::local_ip().await?;

            response.add_answer(
                question.name(),
                net::dns::RecordType::A,
                net::dns::Class::IN,
                5 * 60,
                &net::dns::ResourceRecordData::Address(ip)
            );
        }

        Ok(())
    }
}


const PARTITION_MOUNTS: &'static [&'static str] = &[
    "/",
    "/boot/firmware"
];

// sudo mount -o remount,ro /

// NOTE: If this fails, then likely there is a file still
// open as writeable by some program.
fn toggle_writeable_fs(writeable: bool) -> Result<()> {
    for path in PARTITION_MOUNTS {
        let status = std::process::Command::new("mount")
            .args(&["-o", if writeable { "remount,rw" } else { "remount,ro" }, path])
            .status()?;
        if !status.success() {
            return Err(err_msg("Failed to remount partition"));
        }
    }

    Ok(())
}

#[derive(Default)]
struct CameraSupervisorInst {
    //
}


#[async_trait]
impl CameraSupervisorService for CameraSupervisorInst {

    async fn Status(
        &self,
        request: rpc::ServerRequest<CameraSupervisorStatusRequest>,
        response: &mut rpc::ServerResponse<CameraSupervisorStatusResponse>
    ) -> Result<()> {
        // response.value = self.status().await?;
        Ok(())
    }

    async fn Run(
        &self,
        request: rpc::ServerRequest<CameraSupervisorRunRequest>,
        response: &mut rpc::ServerResponse<CameraSupervisorRunResponse>
    ) -> Result<()> {
        response.value = crate::run::run_command(&request)?;
        Ok(())
    }

    async fn Update(
        &self,
        mut req_stream: rpc::ServerStreamRequest<UpdateRequest>,
        res_stream: &mut rpc::ServerStreamResponse<UpdateResponse>
    ) -> Result<()> {

        // TODO: Acquire an instance wide lock to prevent duplicate concurrent updates.

        {
            let req = req_stream.recv().await?
                .ok_or_else(|| Error::from(rpc::Status::invalid_argument("No first request")))?;

            if !req.has_start_update() {
                return Err(rpc::Status::invalid_argument("Expected first request to be a start_update").into());
            }
        }

        // TODO: Always disable this if this future is dropped.
        toggle_writeable_fs(true)?;

        let data_path = LocalPath::new("/opt/mocap/supervisor/update/data");

        file::create_dir_all(data_path.parent().unwrap()).await?;
        // Clearing any old update.
        file::write(&data_path, &[]).await?;

        res_stream.send(UpdateResponse::default()).await?;

        while let Some(req) = req_stream.recv().await? {
            if !req.payload_chunk().is_empty() {
                file::append(&data_path, req.payload_chunk()).await?;

                res_stream.send(UpdateResponse::default()).await?;
                continue;
            }

            if req.commit_deb() {

                let mut child = std::process::Command::new("dpkg")
                    .arg("-i").arg(&data_path)
                    // Fully isolated so we can update the supervisor itself.
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .process_group(0) 
                    .spawn()?;
                
                let status = child.wait()
                    .map_err(|_| Error::from(rpc::Status::internal("Failed while waiting for dpkg")))?;

                if !status.success() {
                    return Err(rpc::Status::unknown("dpkg -i failed").into());
                }

                let mut res = UpdateResponse::default();
                res.set_commited(true);
                res_stream.send(res).await?;
                break;
            }

            if req.has_commit_image() {

                /*
                treat as a sorted tar.gz
                suppress 
                */
                
            }

            return Err(rpc::Status::invalid_argument("Unsupported command type").into());
        }

        if let Err(e) = toggle_writeable_fs(false) {
            eprintln!("Failed to mark FS as read only: {}", e);
        }

        Ok(())
    }
}


// TODO: Probably just limit this to one thread.
#[executor_main]
async fn main() -> Result<()> {

    let camera_id = {
        let hex = file::read_to_string("/etc/machine-id").await?;
        let data = base_radix::hex_decode(hex.trim())?;
        let id = normalize_entity_id(u64::from_be_bytes(*array_ref![data, 0, 8]));
        entity_id_to_string(id).unwrap()
    };

    println!("Camera Id: {}", camera_id);

    // Normalize initial state which may be wrong if the supervisor restarted mid update.
    if let Err(e) = toggle_writeable_fs(false) {
        eprintln!("Failed to mark FS as read only: {}", e);
    }

    // TODO: Call gethostname/sethostname, set /etc/hostname, etc.

    let service = RootResource::new();

    // TODO: The join_multicast_v4 in here will currently fail with '[ENODEV] No such device' if we haven't gotten an IP address yet.
    let dns_server = net::dns::Server::create_multicast_insecure(Arc::new(DNSServerHandler {
        camera_id: camera_id.clone()
    })).await?;
    service.register_dependency(Arc::new(dns_server)).await;


    let mut rpc_server = rpc::Http2Server::new(Some(81));
    let inst = Arc::new(CameraSupervisorInst::default());
    rpc_server.add_service(inst.clone().into_service())?;
    service.register_dependency(rpc_server.start()).await;

    println!("Running...");
    service.wait().await
}