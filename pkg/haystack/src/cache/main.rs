use std::sync::{Arc};
use std::{time, thread};

use common::errors::*;

use crate::directory::Directory;
use crate::cache::machine::*;
use crate::http_utils::run_http_server;
use crate::cache::routes;

pub async fn run(dir: Directory, port: u16) -> Result<()> {
	// TODO: Whenever possible, re-use the ids of previously existing but now dead machines
	let machine = CacheMachine::load(dir, port)?;
	let mac_ctx = MachineContext::from(machine);

	let mac_handle = Arc::new(mac_ctx);

	CacheMachine::start(&mac_handle).await;

	run_http_server(
		port,
		routes::handle_request,
		mac_handle.clone(),
	).await;

	mac_handle.thread.stop().await;

	// TODO: Use an async sleep.
	// Wait for a small amount of time after we've been marked as not-ready in case stray requests are still pending
	let dur = time::Duration::from_millis(500);
	thread::sleep(dur);

	Ok(())
}
