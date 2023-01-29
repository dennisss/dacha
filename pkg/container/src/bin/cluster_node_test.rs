//! Test suite for an independent cluster_node binary.

#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;

use container::ContainerNodeStub;
use protobuf::text::ParseTextProto;

#[executor_main]
async fn main() -> Result<()> {
    //

    let mut worker_req = container::StartWorkerRequest::default();
    worker_req.spec_mut().set_name("adder_server");

    worker_req
        .spec_mut()
        .add_args(project_path!("target/release/adder_server").to_string());
    worker_req.spec_mut().add_args("--port=rpc".into());

    let mut port = container::WorkerSpec_Port::default();
    port.set_name("rpc");
    port.set_number(30010u32);
    worker_req.spec_mut().add_ports(port);

    let channel = Arc::new(rpc::Http2Channel::create("http://127.0.0.1:10400")?);

    let stub = container::ContainerNodeStub::new(channel);

    let ctx = rpc::ClientRequestContext::default();

    // let res = stub.StartWorker(&ctx, &worker_req).await.result?;
    // println!("{:?}", res);

    let res = stub
        .ListWorkers(&ctx, &container::ListWorkersRequest::default())
        .await
        .result?;
    println!("{:?}", res);

    let mut request = container::GetEventsRequest::default();
    request.set_worker_name("adder_server");

    let res = stub.GetEvents(&ctx, &request).await.result?;
    println!("{:?}", res);

    let mut log_request = container::LogRequest::default();
    log_request.set_attempt_id(1673062714005093 as u64);
    log_request.set_worker_name("adder_server");
    log_request.set_start_offset(0u64);

    let mut log = stub.GetLogs(&ctx, &log_request).await;

    while let Some(entry) = log.recv().await {
        println!("{:?}", entry);
    }

    log.finish().await?;

    /*
    Tests to perform:
    - Use a temp directory for the cluster node data.
    - Basic loading of an adder_server with a binary.
    - Uploading of a blob
    - Test stopping the server (graceful shutdown and non-graceful shutdown)
    - Test reading the log
    - Test restarting on failures (up to N times)
    - Test a server naturally ending and we don't need to restart it.
    - Test the server getting a signal
    - Test with different revisions in the StartWorkerRequest
    - Test having a persistent task
    - Test recording and reading back worker events (especially across resets)
    - Test adding a worker, running it a bit, stopping the worker, and then adding it again (should have a continous chain ofevents)
    - Test wiring up to a fake metastore
    - Hard Killing the root node process should kill all the descendant procesed
    - Soft Killing the root node process should gracefully shut down all descendant containers.

    - Test that we can use the adder_server to simulate 10% utilization of the CPU and measure it.
    */

    Ok(())
}
