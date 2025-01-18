use std::{sync::Arc, time::Duration};

use base_error::*;
use executor::{
    lock_async,
    sync::{AsyncMutex, AsyncRwLock},
};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use labeler_proto::labeler::*;

use crate::renderer::LabelRenderer;

pub struct LabelerImpl {
    task_resource: TaskResource,
    renderer: LabelRenderer,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(LabelerImpl, task_resource);

#[derive(Default)]
struct Shared {
    state: AsyncRwLock<State>,
}

#[derive(Default)]
struct State {
    makers: Vec<MakerEntry>,
}

struct MakerEntry {
    id: String,
    short_name: String,
    inst: AsyncMutex<ptouch::LabelMaker>,
    latest_status: ptouch::Status,
}

impl LabelerImpl {
    pub async fn create() -> Result<Self> {
        let shared = Arc::new(Shared::default());

        let usb_context = usb::Context::create()?;

        // TODO: Have more graceful interruption.
        let task_resource = TaskResource::spawn_interruptable(
            "Labeler::run()",
            Self::run(shared.clone(), usb_context),
        );

        Ok(Self {
            task_resource,
            shared,
            renderer: LabelRenderer::create().await?,
        })
    }

    async fn run(shared: Arc<Shared>, usb_context: usb::Context) -> Result<()> {
        loop {
            let devices = usb_context.enumerate_devices().await?;

            lock_async!(state <= shared.state.write().await?, {
                for device in devices {
                    if !ptouch::LabelMaker::is_supported_device(&device)? {
                        continue;
                    }

                    let mut found_existing = false;
                    for entry in &state.makers {
                        if entry.id == device.sysfs_dir().as_str() {
                            found_existing = true;
                            break;
                        }
                    }

                    if !found_existing {
                        let maker = match Self::create_new_maker(device).await {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("Failed to open label maker: {}", e);
                                continue;
                            }
                        };

                        state.makers.push(maker);
                    }
                }

                let mut i = 0;
                while i < state.makers.len() {
                    let status = lock_async!(inst <= state.makers[i].inst.lock().await?, {
                        inst.get_status().await.map(|v| (v, inst.short_name()))
                    });

                    match status {
                        Ok((status, short_name)) => {
                            state.makers[i].latest_status = status;
                            state.makers[i].short_name = short_name;
                            i += 1;
                        }
                        Err(e) => {
                            eprintln!("Failed to get updated status: {}", e);
                            state.makers.remove(i);
                        }
                    }
                }

                Ok::<(), Error>(())
            })?;

            executor::sleep(Duration::from_secs(10)).await?;
        }
    }

    async fn create_new_maker(device_entry: usb::DeviceEntry) -> Result<MakerEntry> {
        let device = device_entry.open().await?;
        let mut inst = ptouch::LabelMaker::open_existing(device).await?;
        let latest_status = inst.get_status().await?;

        Ok(MakerEntry {
            id: device_entry.sysfs_dir().as_str().to_string(),
            short_name: inst.short_name(),
            inst: AsyncMutex::new(inst),
            latest_status,
        })
    }

    async fn list_devices_impl(&self, request: &ListDevicesRequest) -> Result<ListDevicesResponse> {
        let mut res = ListDevicesResponse::default();

        let state = self.shared.state.read().await?;

        for entry in &state.makers {
            let proto = res.new_devices();
            proto.set_id(entry.id.as_str());
            proto.set_name(entry.short_name.as_str());

            if let Some(tape) = entry.latest_status.tape() {
                let proto = proto.tape_mut();
                proto.set_name(tape.name);
                proto.set_width(tape.width as u32);
                proto.set_print_area(tape.print_area as u32);
                proto.set_margin(tape.margin as u32);
                proto.set_dpi(tape.dpi as u32);
            }
        }

        Ok(res)
    }

    async fn print_impl(&self, request: &PrintLabelRequest) -> Result<PrintLabelResponse> {
        // TODO: Clean up the definition of 'width' in this codebase since sometimes
        // (vertical vs horizontal size. print area vs overall tape size).

        let state = self.shared.state.read().await?;

        let entry = state
            .makers
            .iter()
            .find(|entry| entry.id == request.device_id())
            .ok_or_else(|| rpc::Status::not_found("No such device found"))?;

        let tape = entry
            .latest_status
            .tape()
            .ok_or_else(|| rpc::Status::invalid_argument("No supported tape loaded"))?;

        let mut res = PrintLabelResponse::default();
        // TODO: Populate the device

        let encoder = image::format::jpeg::encoder::JPEGEncoder::new(100);

        let mut rendered_pages = vec![];

        for page in request.pages() {
            let img = self.renderer.render_page(page.as_ref(), &tape)?;

            let mut out = vec![];
            encoder.encode(&img, &mut out)?;

            let proto = res.new_page_images();
            proto.set_data(out);
            proto.set_width(img.width() as u32);
            proto.set_height(img.height() as u32);

            for _ in 0..page.quantity() {
                rendered_pages.push(img.clone());
            }
        }

        if !request.dry_run() {
            // TODO: Make non-interruptable.
            lock_async!(inst <= entry.inst.lock().await?, {
                inst.print(&rendered_pages).await
            })?;
        }

        Ok(res)
    }
}

#[async_trait]
impl LabelerService for LabelerImpl {
    async fn ListDevices(
        &self,
        request: rpc::ServerRequest<ListDevicesRequest>,
        response: &mut rpc::ServerResponse<ListDevicesResponse>,
    ) -> Result<()> {
        response.value = self.list_devices_impl(&request.value).await?;
        Ok(())
    }

    async fn Print(
        &self,
        request: rpc::ServerRequest<PrintLabelRequest>,
        response: &mut rpc::ServerResponse<PrintLabelResponse>,
    ) -> Result<()> {
        response.value = self.print_impl(&request.value).await?;
        Ok(())
    }
}
