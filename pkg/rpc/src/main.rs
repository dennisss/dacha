#![feature(async_closure)]
#![feature(fn_traits)]
#[macro_use]
extern crate common;
extern crate rpc;

mod proto;

use common::async_std::task;
use common::errors::*;
use proto::adder::*;
use protobuf::service::Service;
use common::bytes::Bytes;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

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

async fn run_server() -> Result<()> {
    let mut server = rpc::RPCServer::new(5000);
    let adder = AdderImpl {};
    let service = adder.into_service();
    server.add_service(service)?;
    server.run().await
}

// TODO: Set server side request timeout.

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

//fn main() -> Result<()> {
//
//	task::block_on(run_client())
//}



//trait Handler<G> {
//	fn handle<'a>(&self, x: &'a mut usize) -> Pin<Box<dyn Future<Output=()> +
// 'a>> where G: 'a;
//}
//
//impl<G: Future<Output=()>, F: Fn(&mut usize) -> G> Handler<G> for F {
//	fn handle<'a>(&self, x: &'a mut usize) -> Pin<Box<dyn Future<Output=()> +
// 'a>> where G: 'a { 		Box::pin(self(x))
//	}
//}

async fn adder(x: &mut usize) {
    let f = async move || {
        *x += 4;
    };

    f().await;
}

//struct HandlerImpl {}
//
//impl Handler<()> for HandlerImpl {
//	fn handle<'a>(&self, x: &'a mut usize) -> Pin<Box<dyn Future<Output=()> +
// 'a>> where (): 'a { 		let f = async move || {
//			*x += 4;
//		};
//
//		Box::pin(f())
//	}
//}

/*
use common::async_fn::AsyncFnMut1;

async fn wrapper<F>(handler: &mut F) -> usize
where
    F: for<'a> AsyncFnMut1<&'a mut usize, Output = ()>,
{
    let mut y = 1;
    handler.call_mut(&mut y).await;
    y
}

async fn dostuff() {
    let mut f = async move |x: &mut usize| {
        *x += 33;
    };

    let mut k: usize = 12;

    f(&mut k).await;

    //	let _ = wrapper(&mut f).await;
}
*/

fn main() {
    let r = task::block_on(run_client());
    println!("{:?}", r);
}
