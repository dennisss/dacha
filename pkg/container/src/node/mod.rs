use std::sync::Arc;

use common::errors::*;

use crate::proto::service::*;
use crate::proto::log::*;
use crate::runtime::ContainerRuntime;

pub struct Node {
    runtime: Arc<ContainerRuntime>
}

impl Node {
    pub async fn create() -> Result<Self> {
        let runtime = ContainerRuntime::create().await?;
        Ok(Self { runtime })
    }

    pub fn run(&self) -> impl std::future::Future<Output=Result<()>> {
        self.runtime.clone().run()
    }
}

#[async_trait]
impl ContainerNodeService for Node {

    async fn Query(&self, request: rpc::ServerRequest<QueryRequest>,
                   response: &mut rpc::ServerResponse<QueryResponse>) -> Result<()> {
        let containers = self.runtime.list().await;
        for container in containers {
            response.add_container(container);
        }

        Ok(())
    }

    async fn Start(&self, request: rpc::ServerRequest<StartRequest>,
                    response: &mut rpc::ServerResponse<StartResponse>) -> Result<()> {
        let config = request.value.config();
        let id = self.runtime.start(config).await?;
        response.value.set_container_id(id);
        Ok(())
    }

    async fn GetLogs(&self, request: rpc::ServerRequest<LogRequest>,
                     response: &mut rpc::ServerStreamResponse<LogEntry>) -> Result<()> {

        println!("GETTING LOGS");

        let container_id = request.container_id();
        let mut log_reader = self.runtime.open_log(container_id).await?;

        loop {
            println!("READ");
            let entry = log_reader.read().await?;
            if let Some(entry) = entry {
                let end_stream = entry.end_stream();

                println!("SEND");
                response.send(entry).await?;
                println!("SEND DONE");

                // TODO: Check that we got an end_stream on all the streams.
                if end_stream {
                    break;
                }

            } else {

                println!("SLEEP");
                // TODO: Replace with receiving a notification.
                common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
            }
        }

        Ok(())
    }
}


