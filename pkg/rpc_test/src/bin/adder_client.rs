#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;
extern crate rpc_test;
extern crate http;

use std::convert::TryInto;
use std::sync::Arc;

use common::async_std::task;
use common::errors::*;
use rpc_test::proto::adder::*;

async fn run_client() -> Result<()> {
    // TODO: Must verify that in HTTP if we get a Content-Length, but we don't
    // read the full length, then we should error out the request.

    // TODO: Specify the gRPC protocal in uri?
    let channel = Arc::new(rpc::Http2Channel::create(
        http::ClientOptions::from_authority("127.0.0.1:5000")?)?);
    let stub = AdderStub::new(channel);

    let mut req = AddRequest::default();
    req.set_x(10);
    req.set_y(6);

    let res = stub.Add(&req.into()).await.result?;

    println!("{}", res.z());

    Ok(())
}

fn main() {
    let r = task::block_on(run_client());
    println!("{:?}", r);
}
