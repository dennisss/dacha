extern crate common;
extern crate container;
extern crate http;
extern crate protobuf;
extern crate rpc;
#[macro_use]
extern crate macros;
extern crate builder;

use std::sync::Arc;

use async_std::task::JoinHandle;
use common::async_std::fs;
use common::async_std::io::ReadExt;
use common::async_std::task;
use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use container::{ContainerNodeStub, TaskSpec, TaskSpec_Port, TaskSpec_Volume, WriteInputRequest};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use rpc::ClientRequestContext;

/*

cluster_ctl apply //

cluster_ctl start_task

*/

/*
async fn run_list(node_addr: &str) -> Result<()> {
    let stub = new_stub(node_addr).await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut query_request = container::QueryRequest::default();

    let mut query_response = stub.Query(&request_context, &query_request).await.result?;
    println!("Response:");
    println!("{:#?}", query_response);

    Ok(())
}
*/

fn main() -> Result<()> {
    todo!()
}
