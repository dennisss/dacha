#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;
extern crate rpc_test;
extern crate http;
#[macro_use]
extern crate macros;

use std::convert::TryInto;
use std::sync::Arc;

use common::async_std::task;
use common::errors::*;
use rpc_test::proto::adder::*;

#[derive(Args)]
struct Args {
    target: String
}

async fn run_client() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    // TODO: Must verify that in HTTP if we get a Content-Length, but we don't
    // read the full length, then we should error out the request.

    // TODO: Specify the gRPC protocal in uri?
    let channel = Arc::new(rpc::Http2Channel::create(
        http::ClientOptions::from_authority(args.target.as_str())?)?);
    let stub = AdderStub::new(channel);

    let mut req = AddRequest::default();
    req.set_x(10);
    req.set_y(6);

    if true {
        let (mut req_stream, mut res_stream) = stub.AddStreaming(&rpc::ClientRequestContext::default()).await;

        loop {
            if !req_stream.send(&req).await {
                break;
            }

            let res = res_stream.recv().await;
            if !res.is_some() {
                break;
            }

            println!("{:?}", res);

            common::wait_for(std::time::Duration::from_secs(1)).await;
        }

        return res_stream.finish().await;


    } else {

    
        let res = stub.Add(&rpc::ClientRequestContext::default(), &req).await.result?;
    
        println!("{}", res.z());
    }



    Ok(())
}

fn main() {
    let r = task::block_on(run_client());
    println!("{:?}", r);
}
