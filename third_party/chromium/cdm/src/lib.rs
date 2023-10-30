#[macro_use]
extern crate cxx;
#[macro_use]
extern crate macros;

mod bindings {
    //! Bindgen produced bindings.

    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(unused)]

    mod raw {
        include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
    }

    pub use raw::root::cdm::*;
}

mod ffi;
mod host;
mod host_state;
mod session_event;

use core::ptr::null;
use std::collections::HashMap;
use std::ffi::CStr;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Once};
use std::time::Duration;

use base_error::*;
pub use bindings::InitDataType;
pub use bindings::MessageType;
pub use bindings::Status;
pub use bindings::{SessionType, SubsampleEntry};
use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
pub use session_event::*;

use crate::host::*;
use crate::host_state::*;

static INIT_MODULE: Once = Once::new();

pub struct DecryptedBlock {
    inner: cxx::UniquePtr<ffi::DecryptedBlockImpl>,
}

unsafe impl Send for ffi::DecryptedBlockImpl {}
unsafe impl Sync for ffi::DecryptedBlockImpl {}

impl DecryptedBlock {
    pub fn new() -> Self {
        Self {
            inner: ffi::NewDecryptedBlockImpl(),
        }
    }

    pub fn get_mut(&mut self) -> &mut [u8] {
        unsafe {
            let decrypted_block = Pin::new_unchecked(&mut *self.inner.pin_mut().Cast());

            // If this is null then it means that we never populated the block with an
            // actual buffer.
            let buffer_ptr = decrypted_block.DecryptedBuffer();
            if buffer_ptr.is_null() {
                return &mut [];
            }

            let buffer = Pin::new_unchecked(&mut *buffer_ptr);
            let size = buffer.Size();
            let data = buffer.Data();

            core::slice::from_raw_parts_mut(data, size as usize)
        }
    }
}

pub struct ContentDecryptionModule {
    host_state: Arc<std::sync::Mutex<HostState>>,
    shared: Arc<Shared>,
    host_event_task: ChildTask<()>,

    session_event_receiver: channel::Receiver<SessionEvent>,
}

struct Shared {
    inner: Mutex<cxx::UniquePtr<ffi::ContentDecryptionModule>>,
}

// This is mainly safe as we always maintain an exclusive lock on the module
// which it is being used and it doesn't retain ownership of any external
// buffers while not being syncronously called.
//
// TODO: Consider pinning all operations that call the CDM methods to a single
// OS thread.
unsafe impl Send for ffi::ContentDecryptionModule {}
unsafe impl Sync for ffi::ContentDecryptionModule {}

impl ContentDecryptionModule {
    pub async fn create() -> Result<Self> {
        INIT_MODULE.call_once(|| {
            let ver = ffi::GetCdmVersion();

            assert!(ver != null());
            let s = unsafe { CStr::from_ptr(ver) }.to_str().unwrap();
            println!("[CDM Version] {}", s);

            ffi::InitializeCdmModule_4();
        });

        let (init_sender, init_receiver) = channel::oneshot::channel();

        let host_state = Arc::new(std::sync::Mutex::new(HostState::new(init_sender)));

        let (event_sender, event_receiver) = channel::unbounded();

        let host = Box::new(HostImpl::new(host_state.clone(), event_sender));

        let mut inst = ffi::CreateCdm(host);
        if inst.is_null() {
            return Err(err_msg("CDM instantiation failed"));
        }

        let inner_pin = unsafe { Pin::new_unchecked(&mut *inst.pin_mut().Get()) };

        inner_pin.Initialize(true, false, false);

        let success = init_receiver
            .recv()
            .await
            .map_err(|()| err_msg("CDM cancelled"))?;
        if !success {
            return Err(err_msg("CDM initialization reported failure"));
        }

        let shared = Arc::new(Shared {
            inner: Mutex::new(inst),
        });

        let (session_event_sender, session_event_receiver) = channel::unbounded();

        let host_event_task = ChildTask::spawn(Self::run_host_event_task(
            shared.clone(),
            event_receiver,
            session_event_sender,
        ));

        Ok(Self {
            host_state,
            shared,
            host_event_task,
            session_event_receiver,
        })
    }

    async fn run_host_event_task(
        shared: Arc<Shared>,
        event_receiver: channel::Receiver<HostEvent>,
        session_event_sender: channel::Sender<SessionEvent>,
    ) {
        loop {
            let e = match event_receiver.recv().await {
                Ok(v) => v,
                Err(_) => break,
            };

            match e {
                HostEvent::SessionEvent(e) => {
                    session_event_sender.send(e).await;
                }
                HostEvent::TimerExpired { context } => {
                    let mut inst = shared.inner.lock().await;
                    inst.pin_mut().TimerExpired(context);
                }
                HostEvent::QueryOutputProtection => {
                    let mut inst = shared.inner.lock().await;
                    unsafe {
                        let inst_pin = Pin::new_unchecked(&mut *inst.pin_mut().Get());
                        inst_pin.OnQueryOutputProtectionStatus(
                            bindings::QueryResult::kQuerySucceeded,
                            bindings::OutputLinkTypes::kLinkTypeHDMI.0,
                            bindings::OutputProtectionMethods::kProtectionNone.0,
                        );
                    }
                }
            }
        }
    }

    pub async fn poll_event(&self) -> Result<SessionEvent> {
        Ok(self.session_event_receiver.recv().await?)
    }

    /// On success, returns the session id.
    pub async fn create_session(
        &self,
        session_type: SessionType,
        init_data_type: InitDataType,
        init_data: &[u8],
    ) -> Result<String> {
        let mut inst = self.shared.inner.lock().await;

        let promise = self.host_state.lock().unwrap().new_promise();

        unsafe {
            let inst_pin = Pin::new_unchecked(&mut *inst.pin_mut().Get());

            inst_pin.CreateSessionAndGenerateRequest(
                promise.id(),
                session_type,
                init_data_type,
                init_data.as_ptr(),
                init_data.len() as u32,
            );
        }

        drop(inst);

        promise.wait_new_session().await
    }

    pub async fn update_session(&self, session_id: &str, response: &[u8]) -> Result<()> {
        let mut inst = self.shared.inner.lock().await;

        let promise = self.host_state.lock().unwrap().new_promise();

        unsafe {
            let inst_pin = Pin::new_unchecked(&mut *inst.pin_mut().Get());

            inst_pin.UpdateSession(
                promise.id(),
                core::mem::transmute::<_, *const i8>(session_id.as_bytes().as_ptr()),
                session_id.as_bytes().len() as u32,
                response.as_ptr(),
                response.len() as u32,
            );
        }

        promise.wait_empty().await
    }

    // TODO: Allow customizing the encryption type.
    pub async fn decrypt(
        &self,
        data: &[u8],
        key_id: &[u8],
        iv: &[u8],
        subsamples: &[SubsampleEntry],
        decrypted_block: &mut DecryptedBlock,
    ) -> Result<()> {
        let mut inst = self.shared.inner.lock().await;

        let mut input_buffer = Box::new(bindings::InputBuffer_2::default());

        input_buffer.data = data.as_ptr();
        input_buffer.data_size = data.len() as u32;

        input_buffer.encryption_scheme = bindings::EncryptionScheme::kCenc;

        input_buffer.key_id = key_id.as_ptr();
        input_buffer.key_id_size = key_id.len() as u32;
        input_buffer.iv = iv.as_ptr();
        input_buffer.iv_size = iv.len() as u32;

        input_buffer.subsamples = subsamples.as_ptr();
        input_buffer.num_subsamples = subsamples.len() as u32;

        input_buffer.pattern = bindings::Pattern::default();

        input_buffer.timestamp = 1;

        let status = unsafe {
            let inst_pin = Pin::new_unchecked(&mut *inst.pin_mut().Get());

            inst_pin.Decrypt(&input_buffer, decrypted_block.inner.pin_mut().Cast())
        };

        if status != Status::kSuccess {
            return Err(format_err!("Decryption failed with status: {:?}", status));
        }

        Ok(())
    }
}
