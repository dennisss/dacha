/*
    This file contains the route code specific for writing to the store
    As the batching stuff gets pretty envolved, it deserves its own file
*/

use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::Mutex;

use crate::http_utils::*;
use crate::proto::service::*;
use crate::store::api::*;
use crate::store::machine::*;
use crate::store::needle::*;
use crate::store::volume::*;
use crate::types::*;

// TODO: Need some limits on the maximum size of a single enedle to ensure that
// it can fit in memory.

pub async fn write_single(
    mac_handle: &MachineContext,
    volume_id: VolumeId,
    key: NeedleKey,
    alt_key: NeedleAltKey,
    cookie: CookieBuf,
    content_length: u64,
    mut body: Box<dyn http::Body>,
) -> Result<http::Response> {
    let mut data = vec![];
    data.reserve_exact(content_length as usize);
    body.read_to_end(&mut data).await?; // TODO: Return BAD_REQUEST for Content-Length mismatches.

    // Quickly lock the machine and get a volume reference
    let vol_handle = {
        let mac = mac_handle.inst.read().await;

        match mac.volumes.get(&volume_id) {
            Some(v) => v.clone(),
            None => {
                return Ok(text_response(
                    http::status_code::NOT_FOUND,
                    "Volume not found",
                ))
            }
        }
    };

    let mut vol = vol_handle.lock().await;

    // TODO: Currently we make no attempt to check if it will overflow the volume
    // after the write
    if !vol.can_write() {
        return Ok(text_response(
            http::status_code::BAD_REQUEST,
            "Volume is out of space and not writeable",
        ));
    }

    perform_append(
        &mac_handle,
        &mut vol,
        NeedleChunkPath {
            volume_id,
            key,
            alt_key,
            cookie,
        },
        content_length,
        &[data.into()],
    )?;

    // TODO: If we could defer this until more sequential requests run, our
    // performance would go up
    vol.flush()?;

    Ok(text_response(http::status_code::OK, "Needle added!"))
}

// TODO: Switch to just taking as input a regular buffer as there's no longer a
// use-case for
fn perform_append(
    mac_handle: &MachineContext,
    vol: &mut PhysicalVolume,
    path: NeedleChunkPath,
    size: u64,
    chunks: &[Bytes],
) -> Result<()> {
    let mut strm = super::stream::ChunkedStream::from(chunks);

    let initial_writeability = vol.can_write_soft();

    vol.append_needle(
        NeedleKeys {
            key: path.key,
            alt_key: path.alt_key,
        },
        path.cookie,
        NeedleMeta { flags: 0, size },
        &mut strm,
    )?;

    let final_writeability = vol.can_write_soft();

    // This write has caused the volume to become near-empty
    if final_writeability != initial_writeability {
        println!(
            "- Volume {} on Machine {}: writeability: {}",
            path.volume_id, mac_handle.id, final_writeability
        );
        mac_handle.thread.notify();
    }

    Ok(())
}

// A type of error returned while performing a request
#[derive(Debug, Fail)]
pub(crate) struct APIError {
    pub code: http::status_code::StatusCode,

    /// NOTE: This isn't sent back to the HTTP client as there is no good method
    /// of doing that without sending a body (which may be misinterpreted by
    /// a client).
    pub message: &'static str,
}

impl APIError {
    pub fn new(code: http::status_code::StatusCode, message: &'static str) -> Self {
        Self { code, message }
    }
}

impl std::fmt::Display for APIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}: {}] {}",
            self.code.as_u16(),
            self.code.default_reason().unwrap_or(""),
            self.message
        )
    }
}

// Internal state of the batch writer
struct WriteBatchState<'a> {
    mac_handle: &'a MachineContext,
    last_volume_id: Option<VolumeId>, // Id of the last volume we looked at
    num_written: usize,               // Number of needles we have appended to physical volumes
    num_flushed: usize,               // Number of needles actually flushed to disk
}

pub async fn write_batch(
    mac_handle: &MachineContext,
    body: Box<dyn http::Body>,
) -> Result<http::Response> {
    let mut state = WriteBatchState {
        mac_handle,
        last_volume_id: None,
        num_written: 0,
        num_flushed: 0,
    };

    // Call this before responding or changing volumes
    async fn flush<'a>(state: &mut WriteBatchState<'a>) -> Result<()> {
        if state.num_flushed == state.num_written {
            return Ok(());
        }

        if let Some(vid) = state.last_volume_id {
            let mac = state.mac_handle.inst.read().await;
            mac.volumes.get(&vid).unwrap().lock().await.flush()?;
            state.num_flushed = state.num_written;
        } else if state.num_flushed != state.num_written {
            // Panic not all flushable
            return Err(err_msg(
                "Somehow we have unflushed needles but no previous volume",
            ));
        }

        Ok(())
    }

    async fn get_volume<'a>(
        state: &mut WriteBatchState<'a>,
        volume_id: VolumeId,
    ) -> Result<Arc<Mutex<PhysicalVolume>>> {
        if let Some(vid) = state.last_volume_id {
            // When switching volumes, we must flush the previous one
            if vid != volume_id {
                flush(state).await?;
            }
        }

        let mac = state.mac_handle.inst.read().await;

        state.last_volume_id = Some(volume_id);

        match mac.volumes.get(&volume_id) {
            Some(v) => Ok(v.clone()),
            None => Err(APIError::new(http::status_code::NOT_FOUND, "Volume not found").into()),
        }
    }

    async fn process_request<'a>(
        state: &mut WriteBatchState<'a>,
        mut body: Box<dyn http::Body>,
    ) -> Result<()> {
        let mut header_buf = [0u8; NEEDLE_CHUNK_HEADER_SIZE];
        loop {
            // Read the needle header.
            if let Err(e) = body.read_exact(&mut header_buf).await {
                if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                    if io_error.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                }

                // TODO: Convert into a client error?
                return Err(e);
            }

            let (needle_path, needle_size) =
                NeedleChunk::read_header(&mut std::io::Cursor::new(&header_buf))?;

            // Read needle data.
            let mut data = vec![];
            data.reserve_exact(needle_size as usize);
            data.resize(needle_size as usize, 0);
            body.read_exact(&mut data).await?;

            let vol_handle = get_volume(state, needle_path.volume_id).await?;

            let mut vol = vol_handle.lock().await;

            // TODO: Ideally to be moved into the perform_append and then run with a
            // normalized error catcher at a higher level
            if !vol.can_write() {
                // TODO: We will likely end up moving this check into the append_needle code (or
                // into perform_append)
                return Err(
                    APIError::new(http::status_code::BAD_REQUEST, "Volume not writeable").into(),
                );
            }

            perform_append(
                &state.mac_handle,
                &mut vol,
                needle_path,
                needle_size,
                &[Bytes::from(data)],
            )?;
        }

        Ok(())
    }

    let processing_result = process_request(&mut state, body).await;

    ///// Contruct the response.
    // TODO: We should basically always return a Json response if possible as that
    // allow the client to partially retry only some needles.

    let mut res = StoreWriteBatchResponse::default();

    if let Err(e) = processing_result {
        if let Some(api_error) = e.downcast_ref::<APIError>() {
            res.error_mut().set_code(api_error.code.as_u16() as u32);
            res.error_mut().set_message(api_error.message);
        } else {
            // TODO: Return this as an anonymous JSON response as well?
            return Err(e);
        }
    }

    // Flush all needles to disk
    flush(&mut state).await?;

    res.set_num_written(state.num_written as u64);

    Ok(json_response(http::status_code::OK, &res))
}
