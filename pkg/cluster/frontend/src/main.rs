#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use base_error::*;
use base_units::format_duration_secs;
use common::bytes::Bytes;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::meta::ObjectMetadataTable;
use cluster_client::ClusterServer;
use cluster_proto::cluster::ObjectMetadata;
use rpc_util::NamedPortArg;
use db_table::query_one;
use db_table::db::ProtobufDBTransaction;
use file::LocalPathBuf;

use cluster_frontend::*;


#[derive(Args)]
struct Args {
    port: NamedPortArg,
    config: LocalPathBuf,
    public_credentials_object_prefix: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    
    let mut config = cluster_proto::cluster::FrontendConfig::default();
    protobuf::text::parse_text_proto(&file::read_to_string(args.config).await?, &mut config)?;

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let public_credentials = Arc::new(ObjectCredentialsLoader::create(client.clone(), &args.public_credentials_object_prefix).await?);
    service.register_dependency(public_credentials.clone()).await;

    let domain_name = {
        // TODO: Verify we also have a wildcard subject alt name.
        let options = public_credentials.server_options().get();
        let cert = options.certificate_auth.identities[0].certificates[0].clone();
        cert.subject().common_name()?.ok_or_else(|| err_msg("Certificate has no common name"))?
    };
    println!("Public Domain Name: {}", domain_name);

    let handler = FrontendHttpHandler::create(config, domain_name, client.clone()).await?;

    {
        let mut options = http::ServerOptions::default();
        options.port = Some(args.port.value());
        options.tls = Some(public_credentials.server_options());
        options.force_http2 = true;
        options.connection_options_v2.protocol_settings
            .set(http::v2::SettingId::MAX_CONCURRENT_STREAMS, 16).unwrap();

        let mut server = http::Server::new(handler, options);

        service.register_dependency(Arc::new(server.start())).await;
    }

    // TODO: Also integrate a regular cluster server for mTLS and health checking.
    /*
    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;
    service.register_dependency(server.start()?).await;
    */


    service.wait().await
}

