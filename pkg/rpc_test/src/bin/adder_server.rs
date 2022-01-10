/*
Testing:
    cargo run --bin adder_server -- --port=8000

    curl --http2-prior-knowledge -v 127.0.0.1:8000
        => Should return HTTP 415

    curl --http2-prior-knowledge -v 127.0.0.1:8000 --header "Content-Type: application/grpc+proto"

*/

#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate rpc;
extern crate rpc_test;
extern crate rpc_util;

use std::sync::Arc;

use common::args::ArgType;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::io::prelude::WriteExt;
use common::async_std::task;
use common::errors::*;
use rpc_test::proto::adder::*;
use rpc_util::{AddHealthEndpoints, AddReflection};

#[derive(Args)]
struct Args {
    port: rpc_util::NamedPortArg,
    request_log: Option<String>,
}

struct AdderImpl {
    log_file: Option<File>,
}

#[async_trait]
impl AdderService for AdderImpl {
    async fn Add(
        &self,
        request: rpc::ServerRequest<AddRequest>,
        response: &mut rpc::ServerResponse<AddResponse>,
    ) -> Result<()> {
        println!("{:?}", request.value);
        response.set_z(request.x() + request.y());
        Ok(())
    }

    async fn AddStreaming(
        &self,
        mut request: rpc::ServerStreamRequest<AddRequest>,
        response: &mut rpc::ServerStreamResponse<AddResponse>,
    ) -> Result<()> {
        while let Some(req) = request.recv().await? {
            println!("{:?}", req);
            let z = req.x() + req.y();

            if let Some(mut file) = self.log_file.as_ref() {
                file.write_all(format!("{} + {} = {}\n", req.x(), req.y(), z).as_bytes())
                    .await?;
                file.flush().await?;
            }

            let mut res = AddResponse::default();
            res.set_z(z);
            response.send(res).await?;
        }

        Ok(())
    }
}

// TODO: Set server side request timeout.
async fn run_server() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let log_file = {
        if let Some(path) = args.request_log {
            Some(
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .await?,
            )
        } else {
            None
        }
    };

    let mut server = rpc::Http2Server::new();
    let adder = AdderImpl { log_file };
    let service = adder.into_service();
    server.add_service(service)?;
    server.add_reflection()?;
    server.add_healthz()?;
    server.set_shutdown_token(common::shutdown::new_shutdown_token());

    println!("Starting on port {}", args.port.value());
    server.run(args.port.value()).await
}

fn main() {
    let r = task::block_on(run_server());
    println!("{:?}", r);
}
