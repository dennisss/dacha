use std::sync::Arc;
use std::{thread, time};

use common::errors::*;

use super::super::directory::Directory;
use super::super::http_utils::*;
use super::machine::*;
use super::routes;

fn on_start(mac_handle: &MachineHandle) {}

fn on_stop(mac_handle: &MachineHandle) {}

pub async fn run(dir: Directory, port: u16, folder: &str) -> Result<()> {
    println!("Store folder: {}", folder);

    let machine = StoreMachine::load(&dir, port, folder)?;
    println!("Starting Haystore Id #{}", machine.id());

    let mac_ctx = MachineContext::from(machine, dir).await;

    let mac_handle = Arc::new(mac_ctx);

    StoreMachine::start(&mac_handle);

    run_http_server(port, routes::handle_request, mac_handle.clone()).await;

    // TODO: Ideally this should start running at the same time as the HTTP2 serving
    // shutting down as we probably need some unified shutdown mechanism for
    // servers.
    mac_handle.thread.stop();

    // TODO: EVenetually generalize this as a lame-duck pattern.

    // Wait for a small amount of time after we've been marked as not-ready in case
    // stray requests are still pending
    let dur = time::Duration::from_millis(500);
    thread::sleep(dur);

    // This will retake ownership of the machine and all volumes and will flush all
    // pending physical volume index records to disk
    let mac_ctx = Arc::try_unwrap(mac_handle)
        .map_err(|_| ())
        .expect("Machine handle not released completely");
    let mac: StoreMachine = mac_ctx.inst.into_inner();
    for (_, v) in mac.volumes.into_iter() {
        let v = Arc::try_unwrap(v)
            .map_err(|_| ())
            .expect("Volume not released");
        v.into_inner().close()?;
    }

    Ok(())
}
