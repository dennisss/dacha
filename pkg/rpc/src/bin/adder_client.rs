#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;

use common::async_std::task;
use common::errors::*;
use protobuf::service::Service;
use rpc::proto::adder::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

async fn run_client() -> Result<()> {
    // TODO: Must verify that in HTTP if we get a Content-Length, but we don't
    // read the full length, then we should error out the request.

    // TODO: Specify the gRPC protocal in uri?
    let channel = Arc::new(rpc::RPCChannel::create("http://127.0.0.1:5000")?);
    let stub = AdderStub::new(channel);

    let mut req = AddRequest::default();
    req.set_x(10);
    req.set_y(6);

    let res = stub.Add(&req).await?;

    println!("{}", res.z());

    Ok(())
}

fn main() {
    let r = task::block_on(run_client());
    println!("{:?}", r);
}