use std::sync::Arc;
use std::time::SystemTime;

use cluster_client::ClusterMetaClient;
use common::errors::*;
use cluster_proto::cluster::*;
use grpc_proto::grpc::reflection::v1alpha::*;

/*
TODO: Finish making this work.

cargo run --bin cluster_cli -- ping c8b1m3g8yneyj.meta.system.worker.home.cluster.internal
*/


#[derive(Args)]
pub struct PingCommand {
    #[arg(positional)]
    address: String,
}

pub async fn run_ping(cmd: PingCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let channel = cluster_client::service::create_rpc_channel(&cmd.address, meta_client.clone()).await?;

    let stub = ServerReflectionStub::new(channel);
    let request_context = rpc::ClientRequestContext::default();
    let (request, mut response) = stub.ServerReflectionInfo(&request_context).await;

    response.recv_head().await;

    let ctx = &response.context().http_response_context.as_ref().unwrap().connection_context;
    let tls = ctx.as_ref().unwrap().tls.as_ref().unwrap();

    let cert = tls.certificate.as_ref().unwrap();

    let cn = cert
        .subject()
        .common_name()?
        .ok_or_else(|| err_msg("Server certificate has no common name"))?;
    println!("Server Name: {}", cn);

    let not_before = SystemTime::from(cert.validity().not_before);
    println!("Not Before: {:?}", not_before);

    let now = SystemTime::now();
    
    let t = now.duration_since(not_before).unwrap();
    println!("Age: {:?}", t);


    // let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);
    // println!("{:?}", cn);


    Ok(())
}
