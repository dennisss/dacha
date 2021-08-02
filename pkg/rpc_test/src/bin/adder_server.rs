#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;
extern crate rpc_test;

use std::sync::Arc;

use common::async_std::task;
use common::errors::*;
use rpc_test::proto::adder::*;

struct AdderImpl {}

#[async_trait]
impl AdderService for AdderImpl {
    async fn Add(&self, request: rpc::ServerRequest<AddRequest>, response: &mut rpc::ServerResponse<AddResponse>) -> Result<()> {
        println!("{:?}", request.value);
        response.set_z(request.x() + request.y());
        Ok(())
    }

    async fn AddStreaming(
        &self,
        mut request: rpc::ServerStreamRequest<AddRequest>,
        response: &mut rpc::ServerStreamResponse<AddResponse>
    ) -> Result<()> {
        while let Some(req) = request.recv().await? {
            println!("{:?}", req);
            let mut res = AddResponse::default();
            res.set_z(req.x() + req.y());
            response.send(res).await?;
        }

        Ok(())
    }
}

// TODO: Set server side request timeout.
async fn run_server() -> Result<()> {
    let mut server = rpc::Http2Server::new();
    let adder = AdderImpl {};
    let service = adder.into_service();
    server.add_service(service)?;
    server.set_shutdown_token(common::shutdown::new_shutdown_token());

    server.run(5000).await
}

fn main() {
    let r = task::block_on(run_server());
    println!("{:?}", r);
}
