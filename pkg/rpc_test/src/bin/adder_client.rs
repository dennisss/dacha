#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate http;
extern crate rpc;
extern crate rpc_test;
#[macro_use]
extern crate macros;
extern crate container;

use std::convert::{TryFrom, TryInto};
use std::sync::Arc;

use common::errors::*;
use container::meta::client::ClusterMetaClient;
use rpc_test::proto::adder::*;

#[derive(Args)]
struct Args {
    target: String,
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "add")]
    Add {
        #[arg(positional)]
        x: i32,

        #[arg(positional)]
        y: i32,
    },

    #[arg(name = "busy_loop")]
    BusyLoop {
        #[arg(positional)]
        cpu_usage: f32,
    },
}

async fn run_client() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    // TODO: Must verify that in HTTP if we get a Content-Length, but we don't
    // read the full length, then we should error out the request.

    let channel = {
        let resolver = container::ServiceResolver::create_with_fallback(&args.target, async move {
            Ok(Arc::new(
                ClusterMetaClient::create_from_environment().await?,
            ))
        })
        .await?;

        Arc::new(rpc::Http2Channel::create(
            http::ClientOptions::from_resolver(resolver),
        )?)
    };

    let stub = AdderStub::new(channel);

    match args.command {
        Command::Add { x, y } => {
            let mut req = AddRequest::default();
            req.set_x(x);
            req.set_y(y);

            let res = stub
                .Add(&rpc::ClientRequestContext::default(), &req)
                .await
                .result?;

            println!("{}", res.z());
        }
        Command::BusyLoop { cpu_usage } => {
            let mut req = BusyLoopRequest::default();
            req.set_cpu_usage(cpu_usage);

            stub.BusyLoop(&rpc::ClientRequestContext::default(), &req)
                .await
                .result?;
        }
    }

    return Ok(());

    let mut req = AddRequest::default();
    req.set_x(10);
    req.set_y(6);

    if true {
        let (mut req_stream, mut res_stream) = stub
            .AddStreaming(&rpc::ClientRequestContext::default())
            .await;

        loop {
            if !req_stream.send(&req).await {
                break;
            }

            let res = res_stream.recv().await;
            if !res.is_some() {
                break;
            }

            println!("{:?}", res);

            executor::sleep(std::time::Duration::from_secs(1)).await;
        }

        return res_stream.finish().await;
    } else {
        let res = stub
            .Add(&rpc::ClientRequestContext::default(), &req)
            .await
            .result?;

        println!("{}", res.z());
    }

    Ok(())
}

fn main() {
    let r = executor::run(run_client()).unwrap();
    println!("{:?}", r);
}
