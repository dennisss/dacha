/// CLI for interacting for RPC servers.
///
/// If using a command that requires reflection, the server will need to enable
/// reflection using rpc_util::AddReflection.
///
/// Commands:
/// - List available services
///  - `rpc ls [addr:port]` where '[addr:port]' can be something like
///    '127.0.0.1:'
///
/// - Call a method (currently just unary requests are supported).
///   - `rpc call [addr:port] [service_name].[method_name] [request_text]`

#[macro_use]
extern crate macros;
extern crate http;

use std::sync::Arc;

use common::args::parse_args;
use common::errors::*;
use grpc_proto::reflection::*;
use protobuf::{DescriptorPool, Message};
use rpc::Channel;

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "ls")]
    List(ListCommand),

    #[arg(name = "call")]
    Call(CallCommand),
}

#[derive(Args)]
struct ListCommand {
    #[arg(positional)]
    addr: String,
}

#[derive(Args)]
struct CallCommand {
    #[arg(positional)]
    addr: String,

    #[arg(positional)]
    method_name: String,

    #[arg(positional)]
    request_text: String,
}

struct ServerReflectionClient {
    stub: ServerReflectionStub,
    request: rpc::ClientStreamingRequest<ServerReflectionRequest>,
    response: rpc::ClientStreamingResponse<ServerReflectionResponse>,
}

impl ServerReflectionClient {
    async fn create(channel: Arc<dyn rpc::Channel>) -> Result<Self> {
        let stub = ServerReflectionStub::new(channel);
        let request_context = rpc::ClientRequestContext::default();

        let (request, response) = stub.ServerReflectionInfo(&request_context).await;

        Ok(Self {
            stub,
            request,
            response,
        })
    }

    async fn call(
        &mut self,
        request: &ServerReflectionRequest,
    ) -> Result<ServerReflectionResponse> {
        if !self.request.send(request).await {
            return Err(err_msg("Early end to request stream"));
        }

        let res = match self.response.recv().await {
            Some(v) => v,
            None => {
                self.response.finish().await?;
                return Err(err_msg("No response received"));
            }
        };

        /*
        reflection_req.close().await;
        reflection_res.finish().await?;
        */

        // TODO: Check that there is no error response.

        Ok(res)
    }
}

struct ServerClient {
    channel: Arc<rpc::Http2Channel>,
    services: Vec<String>,
    descriptor_pool: DescriptorPool,
}

impl ServerClient {
    async fn create(addr: &str) -> Result<Self> {
        let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
            &format!("http://{}", addr).parse()?,
        )?)?);

        let mut reflection = ServerReflectionClient::create(channel.clone()).await?;

        let services = {
            let mut req = ServerReflectionRequest::default();
            req.set_list_services("*");

            let res = reflection.call(&req).await?;

            res.list_services_response()
                .service()
                .iter()
                .map(|s| s.name().to_string())
                .collect::<Vec<_>>()
        };

        let descriptor_pool = protobuf::DescriptorPool::new();
        for service in &services {
            let mut req = ServerReflectionRequest::default();
            req.set_file_containing_symbol(service);

            let res = reflection.call(&req).await?;

            for file in res.file_descriptor_response().file_descriptor_proto() {
                descriptor_pool.add_file(file.as_ref())?;
            }
        }

        Ok(Self {
            channel,
            services,
            descriptor_pool,
        })
    }
}

async fn run_ls(cmd: ListCommand) -> Result<()> {
    let mut client = ServerClient::create(&cmd.addr).await?;

    for name in &client.services {
        println!("{}", name);
    }

    Ok(())
}

async fn run_call(cmd: CallCommand) -> Result<()> {
    let mut client = ServerClient::create(&cmd.addr).await?;

    let method_parts = cmd.method_name.split('.').collect::<Vec<_>>();
    let service_suffix = method_parts[0..(method_parts.len() - 1)].join(".");
    let method_name = *method_parts.last().unwrap();

    let mut selected_service = None;
    let mut method_index = 0;

    // Must find a service name and a
    for service_name in &client.services {
        if !service_name.ends_with(&service_suffix) {
            continue;
        }

        let service = client
            .descriptor_pool
            .find_relative_type("", &service_name)
            .and_then(|t| t.to_service())
            .ok_or_else(|| {
                format_err!(
                    "Failed to find descriptor for service named: {}",
                    service_name
                )
            })?;

        for i in 0..service.method_len() {
            if service.method(i).unwrap().proto().name() == method_name {
                if selected_service.is_some() {
                    return Err(format_err!(
                        "Multiple methods matching: {}",
                        cmd.method_name
                    ));
                }

                selected_service = Some(service.clone());
                method_index = i;
            }
        }
    }

    let service = selected_service.ok_or_else(|| err_msg("Failed to find method"))?;
    let method = service.method(method_index).unwrap();

    let mut request_message = protobuf::DynamicMessage::new(
        method
            .input_type()
            .ok_or_else(|| err_msg("Missing input descriptor"))?,
    );

    let mut response_message = protobuf::DynamicMessage::new(
        method
            .output_type()
            .ok_or_else(|| err_msg("Missing output descriptor"))?,
    );

    protobuf::text::parse_text_proto(&cmd.request_text, &mut request_message)?;

    let mut request_context = rpc::ClientRequestContext::default();

    let (mut req, mut res) = client
        .channel
        .call_raw(service.name(), method.proto().name(), &request_context)
        .await;

    let mut req = req.into::<protobuf::DynamicMessage>();

    assert!(req.send(&request_message).await);
    req.close().await;

    let response_bytes = res.recv_bytes().await.unwrap();
    response_message.parse_merge(&response_bytes)?;

    println!(
        "{}",
        protobuf::text::serialize_text_proto(&response_message)
    );

    Ok(())
}

async fn run() -> Result<()> {
    let mut args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::List(cmd) => run_ls(cmd).await,
        Command::Call(cmd) => run_call(cmd).await,
    }
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
