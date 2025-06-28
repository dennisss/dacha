/*
cargo run --bin builder -- build //pkg/cluster/test:app

cargo run --bin cluster_adder_server -- --port=8000
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::{sync::Arc, time::Duration};
use std::time::Instant;

use common::{errors::*, io::Readable};
use executor::bundle::TaskResultBundle;
use executor_multitask::RootResource;
use rpc_test::proto::adder::AdderIntoService;

use base_error::*;
use file::LocalPathBuf;
use http::{
    static_file_handler::{StaticFileBody, StaticFileHandler},
    ServerHandler,
};
use rpc_util::NamedPortArg;
use cluster_client::{ClusterServer, ClusterMetaClient};

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/null"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/Adder/Add"
            is_directory: false
            principals: ["authenticated"]
        }
    ]
"#;


struct NullBody {}

#[async_trait]
impl http::Body for NullBody {
    fn len(&self) -> Option<usize> {
        None
    }

    fn has_trailers(&self) -> bool {
        false
    }

    async fn trailers(&mut self) -> Result<Option<http::Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for NullBody {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        executor::sleep(Duration::from_secs(1)).await?;
        for i in 0..out.len() {
            out[i] = 0;
        }

        Ok(out.len())
    }
}

async fn null_handler(request: http::Request) -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::OK)
        .body(Box::new(NullBody {}))
        .build()
        .unwrap()
}

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    println!("Starting...");
    let start_time = Instant::now();

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client)?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Adder".into(),
        script_path: "built/pkg/cluster/test/app.js".into(),
        vars: None,
    }).await?;
    server.add_request_handler("/", false, web_handler)?;


    server.add_service(rpc_test::AdderImpl::create(None).await?.into_service())?;
    server.add_request_handler("/null", false, http::HttpFn(null_handler));

    service.register_dependency(server.start()?).await;

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
