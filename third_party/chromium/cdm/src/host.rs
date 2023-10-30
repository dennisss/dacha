// Implementation of the Host interface which gets called by C++.
//
// This uses the std::sync::Mutex given the C++ interface is syncronous.
// Operations that need to be async are passed out of this file via a
// 'HostEvent' channel.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{ffi::c_char, os::raw::c_void};

use base_error::*;
use executor::channel;
use executor::channel::oneshot;
use executor::child_task::ChildTask;

use crate::bindings::{Exception, KeyInformation, KeyStatus, MessageType, Time};
use crate::host_state::*;
use crate::session_event::*;

pub enum HostEvent {
    SessionEvent(SessionEvent),
    TimerExpired { context: u64 },
    QueryOutputProtection,
}

/// Implementation of the C++ cdm::Host_10 interface.
/// All calls to that interface in C++ are forwarded to an instance of this
/// struct.
pub struct HostImpl {
    shared: Arc<HostImplShared>,
}

struct HostImplShared {
    state: Arc<Mutex<HostState>>,
    event_sender: channel::Sender<HostEvent>,

    // TODO: Replace with a more standardized child task bundle implementation. Maybe one that uses
    // a slab to get task ids.
    timers: Mutex<HashMap<u64, ChildTask>>,
}

impl HostImpl {
    pub fn new(state: Arc<Mutex<HostState>>, event_sender: channel::Sender<HostEvent>) -> Self {
        Self {
            shared: Arc::new(HostImplShared {
                state,
                event_sender,
                timers: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn resolve_promise(&self, promise_id: u32, result: Result<PromiseValue>) {
        let mut state = self.shared.state.lock().unwrap();
        state.resolve_promise(promise_id, result);
    }

    fn get_str(data: *const c_char, size: u32) -> &'static str {
        std::str::from_utf8(Self::get_slice(data, size)).unwrap()
    }

    fn get_slice(data: *const c_char, size: u32) -> &'static [u8] {
        unsafe {
            core::mem::transmute::<_, &[u8]>(core::slice::from_raw_parts(data, size as usize))
        }
    }

    pub unsafe fn SetTimer(&self, delay_ms: u64, context: u64) {
        println!("[CDM] SetTimer for {}", delay_ms);

        let mut timers = self.shared.timers.lock().unwrap();

        if timers.contains_key(&context) {
            eprintln!("[CDM] Duplicate timer with context: {}", context);
            return;
        }

        let shared = Arc::downgrade(&self.shared);
        timers.insert(
            context,
            ChildTask::spawn(async move {
                executor::sleep(Duration::from_millis(delay_ms)).await;

                let shared = match shared.upgrade() {
                    Some(v) => v,
                    None => return,
                };

                let mut timers = shared.timers.lock().unwrap();
                let _ = shared
                    .event_sender
                    .try_send(HostEvent::TimerExpired { context });
                // This will cancel the current task. TODO: Verify this works.
                timers.remove(&context);
            }),
        );
    }

    pub fn GetCurrentWallTime(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
    }

    pub fn OnInitialized(&self, success: bool) {
        let mut state = self.shared.state.lock().unwrap();

        let sender = match state.init_sender.take() {
            Some(v) => v,
            None => {
                eprintln!("[CDM] Already initialized: {}", success);
                return;
            }
        };

        let _ = sender.send(success);
    }

    pub fn OnResolveKeyStatusPromise(&self, promise_id: u32, key_status: KeyStatus) {
        self.resolve_promise(promise_id, Ok(PromiseValue::KeyStatus(key_status)));
    }

    pub unsafe fn OnResolveNewSessionPromise(
        &self,
        promise_id: u32,
        session_id: *const c_char,
        session_id_size: u32,
    ) {
        let session_id = Self::get_str(session_id, session_id_size);
        self.resolve_promise(
            promise_id,
            Ok(PromiseValue::NewSession(session_id.to_string())),
        );
    }

    pub fn OnResolvePromise(&self, promise_id: u32) {
        self.resolve_promise(promise_id, Ok(PromiseValue::Empty));
    }

    pub unsafe fn OnRejectPromise(
        &self,
        promise_id: u32,
        exception: Exception,
        system_code: u32,
        error_message: *const c_char,
        error_message_size: u32,
    ) {
        let error_message = Self::get_str(error_message, error_message_size);

        self.resolve_promise(
            promise_id,
            Err(format_err!(
                "[CDM Rejection] [Exception: {:?}] [System Code: {}] {}",
                exception,
                system_code,
                error_message
            )),
        );
    }

    pub unsafe fn OnSessionMessage(
        &self,
        session_id: *const c_char,
        session_id_size: u32,
        message_type: MessageType,
        message: *const c_char,
        message_size: u32,
    ) {
        let session_id = Self::get_str(session_id, session_id_size);
        let message = Self::get_slice(message, message_size);

        let _ = self.shared.event_sender.try_send(HostEvent::SessionEvent(
            SessionEvent::SessionMessage {
                message_type,
                session_id: session_id.to_string(),
                message: message.to_vec(),
            },
        ));
    }

    pub unsafe fn OnSessionKeysChange(
        &self,
        session_id: *const c_char,
        session_id_size: u32,
        has_additional_usable_key: bool,
        keys_info: *const KeyInformation,
        keys_info_count: u32,
    ) {
        let session_id = Self::get_str(session_id, session_id_size);

        let keys_info = unsafe { core::slice::from_raw_parts(keys_info, keys_info_count as usize) };

        let mut keys = vec![];
        for key_info in keys_info {
            keys.push(KeyInfo {
                key_id: unsafe {
                    core::slice::from_raw_parts(key_info.key_id, key_info.key_id_size as usize)
                }
                .to_vec(),
                status: key_info.status,
                system_code: key_info.system_code,
            });
        }

        let _ = self.shared.event_sender.try_send(HostEvent::SessionEvent(
            SessionEvent::SessionKeysChange {
                session_id: session_id.to_string(),
                has_additional_usable_key,
                keys,
            },
        ));
    }

    pub unsafe fn OnExpirationChange(
        &self,
        session_id: *const c_char,
        session_id_size: u32,
        new_expiry_time: f64,
    ) {
        let session_id = Self::get_str(session_id, session_id_size);

        let _ = self.shared.event_sender.try_send(HostEvent::SessionEvent(
            SessionEvent::ExpirationChange {
                session_id: session_id.to_string(),
                new_expiry_time,
            },
        ));
    }

    pub unsafe fn OnSessionClosed(&self, session_id: *const c_char, session_id_size: u32) {
        let session_id = Self::get_str(session_id, session_id_size);

        let _ = self.shared.event_sender.try_send(HostEvent::SessionEvent(
            SessionEvent::SessionClosed {
                session_id: session_id.to_string(),
            },
        ));
    }

    pub fn QueryOutputProtectionStatus(&self) {
        let _ = self
            .shared
            .event_sender
            .try_send(HostEvent::QueryOutputProtection);
    }
}
