#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;
use std::sync::Arc;

use common::errors::*;
use executor_multitask::RootResource;
use cluster_client::id::{entity_id_to_string, normalize_entity_id};

// use mocap_proto::mocap::MocapCameraIntoService;

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

    // TODO: Call gethostname/sethostname, set /etc/hostname, etc.

    let service = RootResource::new();

    // TODO: The join_multicast_v4 in here will currently fail with '[ENODEV] No such device' if we haven't gotten an IP address yet.
    let dns_server = net::dns::Server::create_multicast_insecure(Arc::new(DNSServerHandler {
        camera_id: camera_id.clone()
    })).await?;
    service.register_dependency(Arc::new(dns_server)).await;

    /*
    RPC server for:
    - UploadFile
    - Install package
    - Upgrade OS
    */

    println!("Running...");
    service.wait().await

}