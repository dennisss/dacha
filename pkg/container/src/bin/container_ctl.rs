extern crate common;
extern crate container;
extern crate protobuf;
extern crate rpc;
extern crate http;
#[macro_use] extern crate macros;

use std::sync::Arc;

use common::errors::*;
use common::async_std::task;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use container::ContainerNodeStub;
use protobuf::text::parse_text_proto;


#[derive(Args)]
enum Args {
    #[arg(name = "list")]
    List,

    #[arg(name = "start")]
    Start,

    #[arg(name = "logs")]
    Logs(LogsCommand)
}

#[derive(Args)]
struct LogsCommand {
    container_id: String
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args {
        Args::List => run_list().await,
        Args::Start => run_start().await,
        Args::Logs(logs_command) => run_logs(logs_command).await
    }
}

async fn new_stub() -> Result<ContainerNodeStub> {
    let channel = Arc::new(rpc::Http2Channel::create(
        http::ClientOptions::from_uri(&"http://127.0.0.1:8080".parse()?)?)?);
    
    let stub = container::ContainerNodeStub::new(channel);

    Ok(stub)
}

async fn run_list() -> Result<()> {
    let stub = new_stub().await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut query_request = container::QueryRequest::default();

    let mut query_response = stub.Query(&request_context, &query_request).await.result?;
    println!("{:#?}", query_response);

    Ok(())
}

async fn run_start() -> Result<()> {
    let stub = new_stub().await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut container_config = container::ContainerConfig::default();
    parse_text_proto(r#"
        process {
            args: ["/usr/bin/bash", "-c", "for i in {1..5}; do echo \"Tick $i\"; sleep 1; done"]
        }
        mounts: [
            {
                destination: "/proc"
                type: "proc"
                source: "proc"
                options: ["noexec", "nosuid", "nodev"]
            },
            {
                destination: "/usr/bin"
                source: "/usr/bin"
                options: ["bind", "ro"]
            },
            {
                destination: "/lib64"
                source: "/lib64"
                options: ["bind", "ro"]
            },
            {
                destination: "/usr/lib"
                source: "/usr/lib"
                options: ["bind", "ro"]
            }
        ]
    "#, &mut container_config)?;

    let mut start_request = container::StartRequest::default();
    start_request.set_config(container_config);

    let start_response = stub.Start(&request_context, &start_request).await.result?;

    println!("Container Id: {}", start_response.container_id());

    let mut log_request = container::LogRequest::default();
    log_request.set_container_id(start_response.container_id());

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    println!("UNBLOCKED");

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        println!("{}", value);
        common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())
}

async fn run_logs(logs_command: LogsCommand) -> Result<()> {
    let stub = new_stub().await?;
    let request_context = rpc::ClientRequestContext::default(); 

    let mut log_request = container::LogRequest::default();
    log_request.set_container_id(&logs_command.container_id);

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        println!("{}", value);
        common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())

    // 5e2e72f7979c54627dc3156c34ffa794

}


fn main() -> Result<()> {
    task::block_on(run())
}