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
use common::errors::*;
use executor_multitask::RootResource;
use rpc_test::proto::adder::AdderIntoService;
use rpc_test::AdderImpl;
use rpc_util::{AddHealthEndpoints, AddReflection};

#[derive(Args)]
struct Args {
    port: rpc_util::NamedPortArg,
    request_log: Option<String>,
}

// TODO: Set server side request timeout.
#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let adder = AdderImpl::create(args.request_log.as_ref().map(|s| s.as_str())).await?;

    let mut server = rpc::Http2Server::new(Some(args.port.value()));
    let service = adder.into_service();
    server.add_service(service)?;
    server.add_reflection()?;
    server.add_healthz()?;

    println!("Starting on port {}", args.port.value());

    let root = RootResource::new();
    root.register_dependency(server.start()).await;
    root.wait().await
}
