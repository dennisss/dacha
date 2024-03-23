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
use std::time::Duration;
use std::time::Instant;

use common::args::ArgType;
use common::errors::*;
use executor::child_task::ChildTask;
use rpc_test::proto::adder::{AddRequest, AdderIntoService, AdderStub};
use rpc_test::AdderImpl;
use rpc_util::{AddHealthEndpoints, AddReflection};

const BLOCK_SIZE: usize = 4 * 1024;

const TARGET_BYTES: usize = 1 * 1024 * 1024;

#[executor_main]
async fn main() -> Result<()> {
    let server = {
        let adder = AdderImpl::create(None).await?;

        let mut server = rpc::Http2Server::new(Some(8000));
        let service = adder.into_service();
        server.add_service(service)?;
        server.add_reflection()?;
        server.add_healthz()?;
        server.start()
    };

    let channel = Arc::new(rpc::Http2Channel::create("http://127.0.0.1:8000").await?);

    // TODO: Have a proper health check on the channel.
    executor::sleep(Duration::from_millis(100)).await?;

    let stub = AdderStub::new(channel);

    let mut req = AddRequest::default();
    req.set_x(10);
    req.set_y(6);

    let (mut req_stream, mut res_stream) = stub
        .AddStreaming(&rpc::ClientRequestContext::default())
        .await;

    req.set_data(vec![0u8; BLOCK_SIZE]);

    let start = Instant::now();

    for i in 0..(TARGET_BYTES / BLOCK_SIZE) {
        if !req_stream.send(&req).await {
            break;
        }

        let res = res_stream.recv().await;
        if !res.is_some() {
            break;
        }
    }

    let end = Instant::now();

    println!("Time: {:?}", end - start);

    // return res_stream.finish().await;

    /*



    if true {

    } else {
        let res = stub
            .Add(&rpc::ClientRequestContext::default(), &req)
            .await
            .result?;

        println!("{}", res.z());
    }


    */

    Ok(())
}
