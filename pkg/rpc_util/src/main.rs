#[macro_use]
extern crate macros;
extern crate http;

use std::sync::Arc;

use common::args::parse_args;
use common::errors::*;
use grpc_proto::reflection::*;

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "ls")]
    List(ListCommand),
}

#[derive(Args)]
struct ListCommand {
    #[arg(positional)]
    addr: String,
}

async fn run() -> Result<()> {
    let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &format!("http://{}", "127.0.0.1:4000").parse()?,
    )?)?);

    let reflection_stub = ServerReflectionStub::new(channel);

    let request_context = rpc::ClientRequestContext::default();

    let (mut reflection_req, mut reflection_res) =
        reflection_stub.ServerReflectionInfo(&request_context).await;

    let mut req = ServerReflectionRequest::default();
    // req.set_list_services("*");
    req.set_file_containing_symbol("Adder");

    if !reflection_req.send(&req).await {
        return Err(err_msg("Early end to request?"));
    }

    let res = match reflection_res.recv().await {
        Some(v) => v,
        None => {
            reflection_res.finish().await?;
            return Err(err_msg("No response received"));
        }
    };

    /*
    reflection_req.close().await;
    reflection_res.finish().await?;
    */

    println!("{:?}", res);

    // Create a channel

    // Create a ServerReflectionService

    // If listing, then ask for all service names.

    // If calling
    // - Ask for descriptor of the server
    // - Find the descriptor in the descriptor pool for the service
    // - Then find the request type
    // - Use that to create a DynamicMessage
    // - Parse from text
    // - send it
    // - Receive binary
    // - Create DynamicMessage from response type.
    // - Parse wire

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
