#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;

use common::async_std::task;
use common::errors::*;
use rpc::proto::adder::*;

struct AdderImpl {}

#[async_trait]
impl AdderService for AdderImpl {
    async fn Add(&self, request: AddRequest) -> Result<AddResponse> {
        println!("{:?}", request);
        let mut res = AddResponse::default();
        res.set_z(request.x() + request.y());
        Ok(res)
    }
}

// TODO: Set server side request timeout.
async fn run_server() -> Result<()> {
    let mut server = rpc::RPCServer::new(5000);
    let adder = AdderImpl {};
    let service = adder.into_service();
    server.add_service(service)?;
    server.run().await
}

fn main() {
    let r = task::block_on(run_server());
    println!("{:?}", r);
}
