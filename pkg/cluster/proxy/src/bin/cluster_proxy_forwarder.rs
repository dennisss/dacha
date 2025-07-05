#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;

use std::sync::Arc;

use base_error::*;
use common::io::{Writeable, Readable};
use cluster_client::ClusterMetaClient;
use cluster_client::service::create_rpc_channel;
use cluster_proxy_proto::cluster::*;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use executor::bundle::TaskResultBundle;

use net::ip::{SocketAddr, IPAddress};
use net::tcp::{TcpStream, TcpListener};

#[derive(Args)]
struct Args {
    /// Local TCP port on which to listen for traffic to forward through the proxy.
    local_port: NamedPortArg,

    /// Address (ip:port) of the server which will proxy traffic. 
    server_addr: String,

    /// Address (ip:port) which the server should forward traffic to.
    target_addr: String,
}

struct Shared {
    stub: Arc<ProxyStub>,
    target_addr: String,
}

async fn connection_thread(shared: Arc<Shared>, stream: TcpStream) {
    if let Err(e) = connection_thread_inner(shared, stream).await {
        eprintln!("Connection failed: {}", e);
    }
}

async fn connection_thread_inner(shared: Arc<Shared>, mut stream: TcpStream) -> Result<()> {
    stream.set_nodelay(true)?;

    let (mut req_stream, mut res_stream) = shared.stub.Socket(&rpc::ClientRequestContext::default()).await;

    let mut first_req = ProxySocketRequest::default();
    first_req.set_protocol(ProxySocketRequest_Protocol::TCP);
    first_req.set_target_addr(&shared.target_addr);
    if !req_stream.send(&first_req).await {
        return Err(err_msg("Failed to send first message"));
    }

    // TODO: Wait for connection event?

    let (mut client_reader, mut client_writer) = stream.split();

    let mut bundle = TaskResultBundle::new();

    bundle.add("Sender", async move {
        let mut buf = vec![0u8; 8192];
        loop {
            let n = client_reader.read(&mut buf[..]).await?;
            if n == 0 {
                break;
            }
    
            let mut req = ProxySocketRequest::default();
            req.set_data(&buf[0..n]);
    
            if !req_stream.send(&req).await {
                break;
            }
        }

        req_stream.close().await;
        Ok(())
    });

    bundle.add("Receiver", async move {
        while let Some(res) = res_stream.recv().await {
            client_writer.write_all(res.data()).await?;
        }
        res_stream.finish().await?;
        Ok(())    
    });

    bundle.join().await?;

    Ok(())
}


async fn server_thread(shared: Arc<Shared>, mut server: TcpListener) -> Result<()> {
    loop {
        let stream = server.accept().await?;

        // Block external requestors.
        if !stream.peer_addr().ip().is_v4() ||
            !stream.peer_addr().ip().as_bytes().starts_with(&[ 127, 0, 0 ]) {
            continue;
        }

        executor::spawn(connection_thread(shared.clone(), stream));
    }

    Ok(())
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let channel = create_rpc_channel(&args.server_addr, client.clone()).await?;

    // TODO: Add to monitored resources
    let stub = Arc::new(ProxyStub::new(channel));

    let shared = Arc::new(Shared {
        stub,
        // TODO: Verify it is a valid address.
        target_addr: args.target_addr,
    });

    let server = TcpListener::bind(
        SocketAddr::new(IPAddress::V4([127,0,0,1]), args.local_port.value())).await?;
    service.spawn_interruptable("TcpServer", server_thread(shared, server)).await;

    service.wait().await
}
