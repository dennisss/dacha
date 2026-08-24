use common::errors::*;


pub struct DNSServerHandler {
    camera_id: String,
}

impl DNSServerHandler {
    pub fn new(camera_id: String) -> Self {
        Self { camera_id }
    }
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
