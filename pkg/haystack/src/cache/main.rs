use super::super::directory::Directory;
use super::machine::*;
use super::super::errors::*;
use super::super::http::start_http_server;
use super::routes;
use std::sync::{Arc};
use std::{time, thread};

fn on_start(mac_handle: &MachineHandle) {
	CacheMachine::start(mac_handle);
}

fn on_stop(mac_handle: &MachineHandle) {
	mac_handle.thread.stop();

	// Wait for a small amount of time after we've been marked as not-ready in case stray requests are still pending
	let dur = time::Duration::from_millis(500);
	thread::sleep(dur);
}


pub fn run(dir: Directory, port: u16) -> Result<()> {
	// TODO: Whenever possible, re-use the ids of previously existing but now dead machines
	let machine = CacheMachine::load(dir, port)?;
	let mac_ctx = MachineContext::from(machine);

	let mac_handle = Arc::new(mac_ctx);

	start_http_server(
		port,
		&mac_handle,
		&routes::handle_request,
		&on_start,
		&on_stop
	);

	Ok(())
}
