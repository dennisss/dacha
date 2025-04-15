use std::time::{Duration, SystemTime};

use common::errors::*;
use cluster_client::ClusterMetaClient;

use crate::utils::WorkerNodeSelector;

#[derive(Args)]
pub struct EventsCommand {
    worker_selector: WorkerNodeSelector,
}

pub async fn run_events(cmd: EventsCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;

    let node = cmd
        .worker_selector
        .connect(meta_client.clone())
        .await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut request = cluster_client::GetEventsRequest::default();
    request.set_worker_name(&cmd.worker_selector.worker_name);

    let mut resp = node
        .service
        .GetEvents(&request_context, &request)
        .await
        .result?;

    struct Attempt<'a> {
        id: u64,
        start_time: SystemTime,
        end_time: Option<SystemTime>,
        exit_status: Option<cluster_client::ContainerStatus>,
        events: Vec<&'a cluster_client::WorkerEvent>,
    }

    resp.events_mut()
        .sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));

    let mut attempts = vec![];

    println!("{:?}", resp);

    // TODO: If the final attempt (or any event) doesn't have a Stopped event, it
    // may still not be running if the event failed to be saved. Need to cross
    // reference with the current state of the worker on the node.
    for event in resp.events() {
        let time = std::time::UNIX_EPOCH + Duration::from_micros(event.timestamp());

        // TODO: Will eventually need to handle StartFailure
        match event.typ_case() {
            cluster_client::WorkerEventTypeCase::Started(_) => attempts.push(Attempt {
                id: event.timestamp(),
                start_time: time,
                end_time: None,
                exit_status: None,
                events: vec![],
            }),
            cluster_client::WorkerEventTypeCase::StartFailure(v) => {
                println!("START FAILURE");

                // TODO: Report v.status() here.

                attempts.push(Attempt {
                    id: event.timestamp(),
                    start_time: time,
                    end_time: Some(time.clone()),
                    exit_status: None,
                    events: vec![],
                })
            }
            cluster_client::WorkerEventTypeCase::Stopped(e) => {
                let last_attempt = attempts.last_mut().unwrap();
                last_attempt.exit_status = Some(e.status().clone());
                last_attempt.end_time = Some(time);
            }
            _ => {}
        }

        // TODO: There may be zero attempts ().
        /// shoudl do some validation here.
        let last_attempt = attempts.last_mut().unwrap();
        last_attempt.events.push(&event);

        // println!("{:?}", event);
    }

    for attempt in attempts {
        println!("{}: {}", attempt.id, time_to_string(&attempt.start_time));
        if let Some(end_time) = attempt.end_time {
            // TODO: This may be none if there was a start failure.
            println!("=> {:?}", attempt.exit_status.unwrap());
        }
    }

    Ok(())
}

fn time_to_string(time: &SystemTime) -> String {
    common::chrono::DateTime::<common::chrono::Local>::from(*time).to_rfc2822()
}
