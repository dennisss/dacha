#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;

use std::sync::Arc;

use base_error::*;
use common::io::{Writeable, Readable};
use cluster_client::ClusterMetaClient;
use cluster_proxy_proto::cluster::*;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use common::futures::try_join;
use net::ip::SocketAddr;
use net::tcp::TcpStream;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/rpc/cluster.Proxy"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

pub struct ProxyServer {}

#[async_trait]
impl ProxyService for ProxyServer {
    async fn Socket(
        &self,
        mut req_stream: rpc::ServerStreamRequest<ProxySocketRequest>,
        res_stream: &mut rpc::ServerStreamResponse<ProxySocketResponse>,
    ) -> Result<()> {
        let mut first_request = req_stream.recv().await?
            .ok_or_else(|| err_msg("No first packet"))?;

        let addr = first_request.target_addr().parse::<SocketAddr>()?;
        
        if first_request.protocol() != ProxySocketRequest_Protocol::TCP {
            return Err(rpc::Status::invalid_argument("Only TCP sockets supported for now.").into());
        }
        
        let mut target_stream = TcpStream::connect(addr).await?;
        target_stream.set_nodelay(true)?;        
        target_stream.write_all(&first_request.data()[..]).await?;

        let (mut target_reader, mut target_writer) = target_stream.split();

        let r: Result<_> = try_join!(
            async move {
                while let Some(req) = req_stream.recv().await? {
                    target_writer.write_all(&req.data()[..]).await?;
                }

                Ok(())
            },
            async move {
                let mut buffer = vec![0u8; 8192];
                loop {
                    let n = target_reader.read(&mut buffer).await?;
                    if n == 0 {
                        break;
                    }
        
                    let mut res = ProxySocketResponse::default();
                    res.set_data(&buffer[0..n]);
                    res_stream.send(res).await?;
                }

                Ok(())
            }
        );

        r?;

        Ok(())
    }
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let client = ClusterMetaClient::create_from_environment().await?;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let service = RootResource::new();

    service.register_dependency(client.clone()).await;

    let inst = ProxyServer {};

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;
    server.add_service(inst.into_service())?;
    service.register_dependency(server.start()?).await;

    service.wait().await
}
