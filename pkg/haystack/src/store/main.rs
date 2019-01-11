use super::super::directory::Directory;
use super::machine::*;
use super::super::errors::*;
use super::super::http::*;
use std::sync::Arc;
use std::{thread, time};
use super::routes;


fn on_start(mac_handle: &MachineHandle) {
	StoreMachine::start(mac_handle);
}

fn on_stop(mac_handle: &MachineHandle) {
	mac_handle.thread.stop();

	// Wait for a small amount of time after we've been marked as not-ready in case stray requests are still pending
	let dur = time::Duration::from_millis(500);
	thread::sleep(dur);
}

pub fn run(dir: Directory, port: u16, folder: &str) -> Result<()> {

	println!("Store folder: {}", folder);

	let machine = StoreMachine::load(dir, port, folder)?;
	println!("Starting Haystore Id #{}", machine.id());

	let mac_ctx = MachineContext::from(machine);

	let mac_handle = Arc::new(mac_ctx);


	start_http_server(
		port,
		&mac_handle,
		&routes::handle_request,
		&on_start,
		&on_stop
	);


	// This will flush all pending physical volume index records to disk
	let mac = mac_handle.inst.lock().unwrap();
	for (_, v) in mac.volumes.iter() {
		v.lock().unwrap().flush()?;
	}

	Ok(())
}