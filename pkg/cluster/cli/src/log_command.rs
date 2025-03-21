use common::errors::*;

use crate::utils::WorkerNodeSelector;

#[derive(Args)]
pub struct LogCommand {
    worker_selector: WorkerNodeSelector,

    /// Id of the attempt from which to look up logs. If not specified, we will
    /// retrieve the logs of the currently running task attempt.
    attempt_id: Option<u64>,

    /// If true, we will look up the previous attempt (or the currently running
    /// one).
    latest_attempt: Option<bool>,
}

pub async fn run_log(cmd: LogCommand) -> Result<()> {
    let creds = cluster_client::credentials::get_cluster_credentials().await?;

    let node = cmd
        .worker_selector
        .connect(Some(creds.client_options()))
        .await?;

    let request_context = rpc::ClientRequestContext::default();

    let mut log_request = cluster_client::LogRequest::default();
    log_request.set_worker_name(&cmd.worker_selector.worker_name);

    if let Some(num) = cmd.attempt_id {
        log_request.set_attempt_id(num);
    }

    if cmd.latest_attempt == Some(true) {
        let mut request = cluster_client::GetEventsRequest::default();
        request.set_worker_name(&cmd.worker_selector.worker_name);

        let mut resp = node
            .service
            .GetEvents(&request_context, &request)
            .await
            .result?;

        for event in resp.events() {
            if event.has_started() && event.timestamp() > log_request.attempt_id() {
                log_request.set_attempt_id(event.timestamp());
            }
        }
    }

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    println!("<End of log>");

    Ok(())
}
