use std::{sync::Arc, time::Duration};

use base_error::*;
use executor::{
    lock_async, lock,
    sync::{AsyncMutex, AsyncRwLock},
};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use peripherals_proto::peripherals::*;
use nordic_tools::usb_radio::USBRadio;

use crate::config::*;


pub struct PeripheralsImpl {
    task_resource: TaskResource,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(PeripheralsImpl, task_resource);

struct Shared {
    state: AsyncMutex<State>,

    // TODO: REmove
    config: BoardConfig,
}

struct State {
    usb_device: PeripheralsDevice,

    // TODO: This may change due to MCU side timeouts so we need to periodically poll
    // for unknown changes (ideally we'd keep track of the state revision on the MCU side).
    peripherals_state: PeripheralsState,
}

impl PeripheralsImpl {
    pub async fn create(config: BoardConfig) -> Result<Self> {
        let (usb_device, peripherals_state) = PeripheralsDevice::create(&config).await?;
        
        let shared = Arc::new(Shared {
            state: AsyncMutex::new(State {
                usb_device,
                peripherals_state
            }),
            config,
        });

        // TODO: Have more graceful interruption.
        let task_resource = TaskResource::spawn_interruptable(
            "PeripheralsImpl::run()",
            Self::run(shared.clone()),
        );

        Ok(Self {
            task_resource,
            shared,
        })
    }

    async fn run(shared: Arc<Shared>) -> Result<()> {
        loop {
            // TODO: Periodically keep the MCU alive.

            /*
            lock_async!(state <= shared.state.lock().await?, {
                {
                    let mut req = PeripheralRequest::default();
                    req.set_peripheral_index(0 as u32);
                    req.set_gpio_level_mut().set_high(high);;
                    state.usb_device.send_request(&req).await?;
                }
                Result::<_, Error>::Ok(())
            })?;
            */

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    async fn execute_impl(&self, request: &ExecuteRequest) -> Result<ExecuteResponse> {
        let mut res = ExecuteResponse::default();

        // TODO: Make Cancel safe.
        lock_async!(state <= self.shared.state.lock().await?, {
            // TODO: It would be problematic if only one of these failed.
            res.set_response(state.usb_device.send_request(request.raw_request()).await?);
            apply_state_change(request.raw_request(), &mut state.peripherals_state)?;
            
            res.set_state(state.peripherals_state.clone());

            Ok::<_, Error>(())
        })?;

        Ok(res)
    }

    async fn run_macro_impl(&self, request: &RunMacroRequest) -> Result<ExecuteResponse> {
        let mut res = ExecuteResponse::default();

        // TODO: Make Cancel safe.
        lock_async!(state <= self.shared.state.lock().await?, {

            let m = self.shared.config.macros().iter().find(|m| m.name() == request.name())
                .ok_or_else(|| err_msg("Unknown macro"))?;

            for cmd in m.commands() {
                // TODO: It would be problematic if only one of these failed.
                res.set_response(state.usb_device.send_request(cmd.request()).await?);
                apply_state_change(cmd.request(), &mut state.peripherals_state)?;
            }
            
            res.set_state(state.peripherals_state.clone());

            Ok::<_, Error>(())
        })?;

        Ok(res)
    }

}

#[async_trait]
impl PeripheralsService for PeripheralsImpl {
    async fn GetConfig(
        &self,
        request: rpc::ServerRequest<GetConfigRequest>,
        response: &mut rpc::ServerResponse<GetConfigResponse>,
    ) -> Result<()> {

        response.value.set_config(self.shared.config.clone());
        lock!(state <= self.shared.state.lock().await?, {
            response.value.set_state(state.peripherals_state.clone());
        });

        Ok(())
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        response.value = self.execute_impl(&request.value).await?;
        Ok(())
    }

    async fn RunMacro(
        &self,
        request: rpc::ServerRequest<RunMacroRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        response.value = self.run_macro_impl(&request.value).await?;
        Ok(())
    }
}
