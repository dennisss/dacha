use std::sync::Arc;

use common::errors::*;
use executor::sync::AsyncVariable;
use executor::lock;
use executor::child_task::ChildTask;
use executor::cancellation::AlreadyCancelledToken;
use executor_multitask::{impl_resource_passthrough, TaskResource, ServiceResource};
use mocap_proto::mocap::*;
use rpc_util::*;

#[derive(Default)]
pub struct AuxRpcServer {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    state: AsyncVariable<State>
}

#[derive(Default)]
struct State {
    server: Option<Server>,
    last_error: Option<String>,
}

struct Server {
    watcher: ChildTask,
    inst: Arc<dyn ServiceResource>
}

impl AuxRpcServer {
    // NOTE: It starts out as not running.
    pub fn create() -> Self {
        let shared = Arc::new(Shared {
            state: Default::default()
        });

        Self {
            shared,
        }
    }
    
    pub async fn status(&self) -> Result<AuxRpcServerStatus> {
        lock!(state <= self.shared.state.lock().await?, {
            let mut out = AuxRpcServerStatus::default();
            if state.server.is_some() {
                out.set_running(true);
            } else if let Some(e) = &state.last_error {
                out.set_error(e);
            }

            Ok(out)
        })
    }

    pub async fn start(&self, service: Arc<dyn rpc::Service>, config: &AuxRpcServerConfig) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if state.server.is_some() {
                return;
            }

            let inst = match self.create_server(service, config) {
                Ok(v) => v,
                Err(e) => {
                    state.last_error = Some(e.to_string());
                    return;
                }
            };

            let watcher = ChildTask::spawn(Self::server_watcher(self.shared.clone(), inst.clone()));

            state.server = Some(Server {
                inst,
                watcher
            });
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if let Some(server) = &state.server {
                server.inst
                    .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()));
            }
        });
        Ok(())
    }

    fn create_server(&self, service: Arc<dyn rpc::Service>, config: &AuxRpcServerConfig) -> Result<Arc<dyn ServiceResource>> {
        let mut server = rpc::Http2Server::new(Some(config.port() as u16));
        server.add_service(service)?;
        server.add_reflection()?;
        server.add_healthz()?;
        Ok(server.start())
    }

    // TODO: Use a weak pointer
    async fn server_watcher(shared: Arc<Shared>, server: Arc<dyn ServiceResource>) {
        let res = server.wait_for_termination(true).await;
        drop(server);

        lock!(state <= shared.state.lock().await.unwrap(), {
            state.last_error = match res {
                Ok(()) => None,
                Err(e) => Some(e.to_string())
            };

            state.server = None;
        });
    }

}