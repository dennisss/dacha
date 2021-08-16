extern crate common;
extern crate container;
extern crate http;
extern crate protobuf;
extern crate rpc;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use async_std::task::JoinHandle;
use common::async_std::fs;
use common::async_std::io::ReadExt;
use common::async_std::task;
use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use container::{ContainerNodeStub, TaskSpec_Port, TaskSpec_Volume, WriteInputRequest};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use rpc::ClientRequestContext;

#[derive(Args)]
enum Args {
    #[arg(name = "list")]
    List,

    #[arg(name = "start")]
    Start,

    #[arg(name = "logs")]
    Logs(LogsCommand),
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
        Args::Logs(logs_command) => run_logs(logs_command).await,
    }
}

async fn new_stub() -> Result<ContainerNodeStub> {
    let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &"http://127.0.0.1:8080".parse()?,
    )?)?);

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
    let stub = Arc::new(new_stub().await?);
    let request_context = rpc::ClientRequestContext::default();

    let tmp_file = "/tmp/container_archive";
    {
        let mut tar_writer = compression::tar::Writer::open(tmp_file).await?;

        let root_dir = common::project_dir().join("target/debug");

        let options = compression::tar::AppendFileOption {
            mask: compression::tar::FileMetadataMask {},
            root_dir: root_dir.clone(),
        };

        tar_writer
            .append_file(&root_dir.join("adder_server"), &options)
            .await?;
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

    // TODO: Interactive exec style runs should be interactive in the sense that
    // when the client's connection is closed, the container should also be
    // killed.

    println!("Starting server");

    // ["/usr/bin/bash", "-c", "for i in {1..20}; do echo \"Tick $i\"; sleep 1;
    // done"]

    let mut terminal_mode = false;

    let mut start_request = container::StartTaskRequest::default();
    // start_request.task_spec_mut().set_name("shell");
    // start_request.task_spec_mut().add_args("/bin/bash".into());
    // start_request.task_spec_mut().add_env("TERM=xterm-256color".into());

    let mut port = TaskSpec_Port::default();
    port.set_name("rpc");
    port.set_number(30001);
    start_request.task_spec_mut().add_ports(port);

    start_request.task_spec_mut().set_name("adder_server");
    start_request
        .task_spec_mut()
        .add_args("/volumes/main/adder_server".into());
    start_request.task_spec_mut().add_args("--port=rpc".into());
    start_request
        .task_spec_mut()
        .add_args("--request_log=/volumes/data/requests".into());

    let mut main_volume = TaskSpec_Volume::default();
    main_volume.set_name("main");
    main_volume.set_blob_id(blob_data.id());
    start_request.task_spec_mut().add_volumes(main_volume);

    let mut adder_volume = TaskSpec_Volume::default();
    adder_volume.set_name("data");
    adder_volume.set_persistent_name("adder_data");
    start_request.task_spec_mut().add_volumes(adder_volume);

    let start_response = stub
        .StartTask(&request_context, &start_request)
        .await
        .result?;

    // TODO: Now wait for the task to enter the Running state.
    // ^ this is required to ensure that we don't fetch logs for a past iteration of
    // the task.

    // println!("Container Id: {}", start_response.container_id());

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(start_request.task_spec().name());

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    if terminal_mode {
        let stdin_task = start_terminal_input_task(
            &stub,
            &request_context,
            start_request.task_spec().name().to_string(),
        )
        .await?;
    }

    // TODO: Currently this seems to never unblock once the connection has been
    // closed.

    let mut stdout = common::async_std::io::stdout();
    while let Some(entry) = log_stream.recv().await {
        // TODO: If we are not in terminal mode, restrict ourselves to only writing out
        // characters that are in the ASCII visible range (so that we can't
        // effect the terminal with escape codes).

        stdout.write_all(entry.value()).await?;
        stdout.flush().await?;
    }

    log_stream.finish().await?;

    if terminal_mode {
        // Always write the terminal reset sequence at the end.
        // TODO: Should should only be needed in
        // TODO: Ensure that this is always written even if the above code fails.
        stdout.write_all(&[0x1b, b'c']).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn start_terminal_input_task(
    stub: &ContainerNodeStub,
    request_context: &ClientRequestContext,
    task_name: String,
) -> Result<JoinHandle<()>> {
    let mut input_req = stub.WriteInput(&request_context).await;

    if !isatty(0)? {
        return Err(err_msg("Expected stdin to be a tty"));
    }

    // A good explanation of these flags is present in:
    // https://viewsourcecode.org/snaptoken/kilo/02.enteringRawMode.html#disable-raw-mode-at-exit

    let mut termios = tcgetattr(0)?;
    // Disable echoing of every input character to the output.
    termios.local_flags.remove(LocalFlags::ECHO);
    // Disable canonical mode: meaning we'll read bytes at a time instead of only
    // reading once an entire line was written.
    termios.local_flags.remove(LocalFlags::ICANON);
    // Disable receiving a signal for Ctrl-C and Ctrl-Z.
    // termios.local_flags.remove(LocalFlags::ISIG);
    // Disable Ctrl-S and Ctrl-Q.
    termios.input_flags.remove(InputFlags::IXON);
    // Disable Ctrl-V.
    termios.local_flags.remove(LocalFlags::IEXTEN);

    termios.input_flags.remove(InputFlags::ICRNL);
    termios.output_flags.remove(OutputFlags::OPOST);

    termios
        .input_flags
        .remove(InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP);
    termios.control_flags |= ControlFlags::CS8;

    tcsetattr(0, nix::sys::termios::SetArg::TCSAFLUSH, &termios)?;

    // TODO: When we create the tty on the server, do we need to explicitly enable
    // all of the above flags.

    Ok(task::spawn(async move {
        let mut stdin = common::async_std::io::stdin();

        loop {
            let mut data = [0u8; 512];

            let n = stdin.read(&mut data).await.expect("Stdin Read failed");
            if n == 0 {
                println!("EOI");
                break;
            }

            let mut input = WriteInputRequest::default();
            input.set_task_name(&task_name);
            input.set_data(data[0..n].to_vec());

            if !input_req.send(&input).await {
                break;
            }
        }

        let res = input_req.finish().await;
        println!("{:?}", res);
    }))
}

async fn run_logs(logs_command: LogsCommand) -> Result<()> {
    let stub = new_stub().await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(&logs_command.task_name);

    let mut log_stream = stub.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
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
