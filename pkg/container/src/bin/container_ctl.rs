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
use common::async_std::fs;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use container::{ContainerNodeStub, TaskSpec_Volume};
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
    task_name: String,
    // container_id: String
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

    let tmp_file = "/tmp/container_archive";
    {
        let mut tar_writer = compression::tar::Writer::open(tmp_file).await?;
        
        let root_dir = common::project_dir().join("target/release");
        
        let options = compression::tar::AppendFileOption {
            mask: compression::tar::FileMetadataMask {},
            root_dir: root_dir.clone()
        };

        tar_writer.append_file(&root_dir.join("adder_server"), &options).await?;
        tar_writer.finish().await?;
    }

    let mut data = fs::read(tmp_file).await?;

    let hash = {
        let mut hasher = SHA256Hasher::default();
        let hash = hasher.finish_with(&data);
        common::hex::encode(hash)
    };

    println!("Uploading blob: {}", hash);

    let mut blob_data = container::BlobData::default();
    blob_data.set_id(hash);
    blob_data.set_data(data);

    let mut request = stub.UploadBlob(&request_context).await;
    request.send(&blob_data).await;

    // TOOD: Catch already exists errors.
    if let Err(e) = request.finish().await {
        let mut ignore_error = false;
        if let Some(status) = e.downcast_ref::<rpc::Status>() {
            if status.code == rpc::StatusCode::AlreadyExists {
                println!("=> {}", status.message);
                ignore_error = true;
            }
        }

        if !ignore_error {
            return Err(e);
        }
    }

    println!("Starting server");

    // ["/usr/bin/bash", "-c", "for i in {1..20}; do echo \"Tick $i\"; sleep 1; done"]

    let mut start_request = container::StartTaskRequest::default();
    start_request.task_spec_mut().set_name("adder_server");
    start_request.task_spec_mut().add_args("/volumes/main/adder_server".into());

    let mut main_volume = TaskSpec_Volume::default();
    main_volume.set_name("main");
    main_volume.set_blob_id(blob_data.id());

    start_request.task_spec_mut().add_volumes(main_volume);

    let start_response = stub.StartTask(&request_context, &start_request).await.result?;

    // println!("Container Id: {}", start_response.container_id());

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(start_request.task_spec().name());

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    // TODO: Currently this seems to never unblock once the connection has been closed.
    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())
}

async fn run_logs(logs_command: LogsCommand) -> Result<()> {
    let stub = new_stub().await?;
    let request_context = rpc::ClientRequestContext::default(); 

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(&logs_command.task_name);

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        println!("...");

        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())

    // 5e2e72f7979c54627dc3156c34ffa794

}


fn main() -> Result<()> {
    task::block_on(run())
}