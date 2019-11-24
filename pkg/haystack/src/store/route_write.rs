/*
	This file contains the route code specific for writing to the store
	As the batching stuff gets pretty envolved, it deserves its own file
*/

use super::super::common::*;
use super::super::errors::*;
use super::super::http::*;
use super::machine::*;
use super::volume::*;
use super::needle::*;
use hyper::{Body, Response, StatusCode};
use std::sync::{Arc, Mutex};
use super::api::*;
use futures::Stream;
use futures::StreamExt;
use futures::future::{ok, err};
use futures::compat::Stream01CompatExt;

pub async fn write_single(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey, cookie: CookieBuf,
	content_length: u64,
	body: Body
) -> Result<Response<Body>> {

	let mut chunks = vec![];
	let mut nread = 0;

	while let Some(Ok(c)) = body.compat().next().await {
		nread = nread + c.len();
		chunks.push(c.into_bytes());
		if nread >= (content_length as usize) {
			break;
		}
	}

	if nread != (content_length as usize) {
		return Ok(text_response(StatusCode::BAD_REQUEST, "Request payload bad length"));
	}

	// Quickly lock the machine and get a volume reference
	let vol_handle = {
		let mac = mac_handle.inst.read().unwrap();
		
		match mac.volumes.get(&volume_id) {
			Some(v) => v.clone(),
			None => return Ok(text_response(StatusCode::NOT_FOUND, "Volume not found")),
		}
	};

	let mut vol = vol_handle.lock().unwrap();

	// TODO: Currently we make no attempt to check if it will overflow the volume after the write
	if !vol.can_write() {
		return Ok(text_response(StatusCode::BAD_REQUEST, "Volume is out of space and not writeable"));
	}

	perform_append(&mac_handle, &mut vol, NeedleChunkPath {
		volume_id, key, alt_key, cookie
	}, content_length, &chunks)?;

	// TODO: If we could defer this until more sequential requests run, our performance would go up
	vol.flush()?;

	Ok(text_response(StatusCode::OK, "Needle added!"))
}

fn perform_append(mac_handle: &MachineHandle, vol: &mut PhysicalVolume, path: NeedleChunkPath, size: u64, chunks: &[bytes::Bytes]) -> Result<()> {

	let mut strm = super::stream::ChunkedStream::from(chunks);

	let initial_writeability = vol.can_write_soft();

	vol.append_needle(
		NeedleKeys { key: path.key, alt_key: path.alt_key },
		path.cookie,
		NeedleMeta { flags: 0, size: size },
		&mut strm
	)?;

	let final_writeability = vol.can_write_soft();

	// This write has caused the volume to become near-empty
	if final_writeability != initial_writeability {
		println!("- Volume {} on Machine {}: writeability: {}", path.volume_id, mac_handle.id, final_writeability);
		mac_handle.thread.notify();
	}


	Ok(())
}


// Internal state of the batch writer 
struct WriteBatchState {
	mac_handle: MachineHandle,

	header: Option<(NeedleChunkPath, NeedleSize)>,
	header_buf: Vec<u8>,
	
	chunks: Vec<bytes::Bytes>,
	nread: usize,

	last_volume_id: Option<VolumeId>, // Id of the last volume we looked at
	num_written: usize, // Number of needles we have appended to physical volumes
	num_flushed: usize  // Number of needles actually flushed to disk
}

pub fn write_batch(
	mac_handle: MachineHandle,
	body: Body
) -> impl std::future::Future<Output=std::result::Result<Response<Body>, Error>> {

	let mut header_buf = vec![]; header_buf.reserve_exact(NEEDLE_CHUNK_HEADER_SIZE);

	let state_handle = Arc::new(Mutex::new(WriteBatchState {
		mac_handle,

		header: None,
		header_buf,

		chunks: vec![],
		nread: 0,

		last_volume_id: None,
		num_written: 0,
		num_flushed: 0
	}));

	// Tries to read bytes for the header returning the tail end of the current chunk
	fn take_header(state: &mut WriteBatchState, data: bytes::Bytes) -> Result<bytes::Bytes> {
		let nleft = NEEDLE_CHUNK_HEADER_SIZE - state.header_buf.len();
		let ntake = std::cmp::min(nleft, data.len());

		state.header_buf.extend_from_slice(&data[0..ntake]);

		// Check if we are done building the header
		if state.header_buf.len() == NEEDLE_CHUNK_HEADER_SIZE {
			state.header = Some(
				NeedleChunk::read_header(&mut std::io::Cursor::new(&state.header_buf))?
			);

			state.header_buf.clear();
			state.nread = 0;
		}

		Ok(data.slice_from(ntake))
	}

	// Call this before responding or changing volumes
	fn flush(state: &mut WriteBatchState) -> Result<()> {
		if state.num_flushed == state.num_written {
			return Ok(());
		}

		if let Some(vid) = state.last_volume_id {
			let mac = state.mac_handle.inst.read().unwrap();
			mac.volumes.get(&vid).unwrap().lock().unwrap().flush()?;
			state.num_flushed = state.num_written;
		}
		else if state.num_flushed != state.num_written {
			// Panic not all flushable
			return Err("Somehow we have unflushed needles but no previous volume".into());
		}

		Ok(())
	}

	fn get_volume(state: &mut WriteBatchState, volume_id: VolumeId) -> Result<Arc<Mutex<PhysicalVolume>>> {

		if let Some(vid) = state.last_volume_id {
			// When switching volumes, we must flush the previous one
			if vid != volume_id {
				flush(state)?;
			}
		}

		let mac = state.mac_handle.inst.read().unwrap();

		state.last_volume_id = Some(volume_id);
		
		match mac.volumes.get(&volume_id) {
			Some(v) => Ok(v.clone()),
			None => {	
				Err(ErrorKind::API(404, "Volume not found").into())
			}
		}
	}

	fn take_chunk(state: &mut WriteBatchState, data: bytes::Bytes, path: NeedleChunkPath, size: NeedleSize) -> Result<bytes::Bytes> {

		let size = size as usize;

		let nleft = (size - state.nread) as usize;
		let ntake = std::cmp::min(nleft, data.len());

		state.chunks.push(data.slice_to(ntake));
		state.nread = state.nread + ntake;

		// Check if we are done reading this chunk
		if state.nread == size {
			
			let vol_handle = get_volume(state, path.volume_id)?;

			let mut vol = vol_handle.lock().unwrap();

			// TODO: Ideally to be moved into the perform_append and then run with a normalized error catcher at a higher level
			if !vol.can_write() {
				// TODO: We will likely end up moving this check into the append_needle code (or into perform_append)
				return Err(ErrorKind::API(400, "Not writeable").into());
			}

			perform_append(&state.mac_handle, &mut  vol, path, size as u64, &state.chunks)?;
			
			state.num_written = state.num_written + 1;
			state.chunks.clear();
			state.header = None;
		}

		Ok(data.slice_from(ntake))
	}

	fn process_data(state: &mut WriteBatchState, mut data: bytes::Bytes) -> Result<()> {
		while data.len() > 0 {
			// Easiest to copy out the header?
			match state.header.clone() {
				None => {
					data = take_header(state, data)?;
				},
				Some((path, size)) => {
					data = take_chunk(state, data, path, size)?;
				}
			};
		}

		Ok(())
	}


	body
	// TODO: How to simulatenously map while responding (issue being that we don't have enough Arcs to the state right?)
	.map_err(|e| e.into())
	.fold(state_handle.clone(), |state_handle, c| {
		{
			let mut state = state_handle.lock().unwrap();
			let data = c.into_bytes();

			if let Err(e) = process_data(&mut state, data) {
				return err(e);
			}
		}

		ok(state_handle)
	})
	.and_then(|state_handle| {
		let state = state_handle.lock().unwrap();

		if state.chunks.len() > 0 {
			// StatusCode::BAD_REQUEST
			return Err(ErrorKind::API(400, "Received less data than we expected").into());
		}

		Ok(())
	})
	// Lastly, construct the response
	.then(move |res| {
		let mut state = state_handle.lock().unwrap();
		let mut error = None;

		if let Err(e) = res {
			if let Error(ErrorKind::API(code, msg), _) = e {
				error = Some(StoreError {
					code,
					message: String::from(msg)
				});
			}
			else {
				return err(e);
			}
		}

		// Flush all needles to disk
		if let Err(e) = flush(&mut state) {
			return err(e);
		}

		ok(json_response(StatusCode::OK, &StoreWriteBatchResponse {
			num_written: state.num_written,
			error
		}))
	})
}