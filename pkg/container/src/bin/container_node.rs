extern crate common;
extern crate container;
extern crate protobuf;
extern crate rpc;

use std::sync::Arc;

use common::errors::*;
use common::async_std::task;
use protobuf::text::parse_text_proto;
use container::ContainerNodeIntoService;

async fn run() -> Result<()> {

    println!("Starting node!");

    let node = container::Node::create().await?;

    let task_handle = task::spawn(node.run());

    let mut server = rpc::Http2Server::new();
    server.add_service(node.into_service());
    server.run(8080).await;

    // TODO: Join the task.

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}