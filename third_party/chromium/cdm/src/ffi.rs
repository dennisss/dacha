use std::os::raw::c_char;

use cxx::ExternType;
use cxx::UniquePtr;

use crate::bindings::{
    Exception, InitDataType, InputBuffer_2, KeyInformation, KeyStatus, MessageType, SessionType,
    Status,
};
use crate::host::HostImpl;

unsafe impl ExternType for Exception {
    type Id = type_id!("cdm::Exception");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for KeyInformation {
    type Id = type_id!("cdm::KeyInformation");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for KeyStatus {
    type Id = type_id!("cdm::KeyStatus");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for MessageType {
    type Id = type_id!("cdm::MessageType");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for InitDataType {
    type Id = type_id!("cdm::InitDataType");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for SessionType {
    type Id = type_id!("cdm::SessionType");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for Status {
    type Id = type_id!("cdm::Status");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for InputBuffer_2 {
    type Id = type_id!("cdm::InputBuffer_2");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for QueryResult {
    type Id = type_id!("cdm::QueryResult");
    type Kind = cxx::kind::Trivial;
}

pub use self::ffi::*;

#[cxx::bridge]
mod ffi {

    #[namespace = "cdm"]
    unsafe extern "C++" {
        // NOTE: Keep in sync with ExternType definitions above.
        type Exception = crate::bindings::Exception;
        type KeyInformation = crate::bindings::KeyInformation;
        type KeyStatus = crate::bindings::KeyStatus;
        type MessageType = crate::bindings::MessageType;
        type InitDataType = crate::bindings::InitDataType;
        type SessionType = crate::bindings::SessionType;
        type Status = crate::bindings::Status;
        type InputBuffer_2 = crate::bindings::InputBuffer_2;
        type QueryResult = crate::bindings::QueryResult;
    }

    extern "Rust" {
        type HostImpl;

        unsafe fn SetTimer(&self, delay_ms: u64, context: u64);

        fn GetCurrentWallTime(&self) -> f64;

        fn OnInitialized(&self, success: bool);

        fn OnResolveKeyStatusPromise(&self, promise_id: u32, key_status: KeyStatus);

        unsafe fn OnResolveNewSessionPromise(
            &self,
            promise_id: u32,
            session_id: *const c_char,
            session_id_size: u32,
        );

        fn OnResolvePromise(&self, promise_id: u32);

        unsafe fn OnRejectPromise(
            &self,
            promise_id: u32,
            exception: Exception,
            system_code: u32,
            error_message: *const c_char,
            error_message_size: u32,
        );

        unsafe fn OnSessionMessage(
            &self,
            session_id: *const c_char,
            session_id_size: u32,
            message_type: MessageType,
            message: *const c_char,
            message_size: u32,
        );

        unsafe fn OnSessionKeysChange(
            &self,
            session_id: *const c_char,
            session_id_size: u32,
            has_additional_usable_key: bool,
            keys_info: *const KeyInformation,
            keys_info_count: u32,
        );

        unsafe fn OnExpirationChange(
            &self,
            session_id: *const c_char,
            session_id_size: u32,
            new_expiry_time: f64,
        );

        unsafe fn OnSessionClosed(&self, session_id: *const c_char, session_id_size: u32);

        fn QueryOutputProtectionStatus(&self);

    }

    unsafe extern "C++" {
        include!("chromium_cdm/src/ffi.h");

        fn InitializeCdmModule_4();

        fn DeinitializeCdmModule();

        fn GetCdmVersion() -> *const c_char;
    }

    #[namespace = "cdm"]
    unsafe extern "C++" {
        type ContentDecryptionModule;

        fn CreateCdm(host: Box<HostImpl>) -> UniquePtr<ContentDecryptionModule>;

        // Return value may be null.
        fn Get(self: Pin<&mut Self>) -> *mut ContentDecryptionModule_10;

        // Wrapped version of ContentDecryptionModule_10::TimerExpired with the context
        // cast to a u64 to pass passing it across threads simpler.
        fn TimerExpired(self: Pin<&mut Self>, context: u64);
    }

    #[namespace = "cdm"]
    unsafe extern "C++" {
        type Buffer;

        fn Data(self: Pin<&mut Self>) -> *mut u8;

        fn Size(&self) -> u32;
    }

    #[namespace = "cdm"]
    unsafe extern "C++" {
        type DecryptedBlock;

        fn DecryptedBuffer(self: Pin<&mut Self>) -> *mut Buffer;
    }

    #[namespace = "cdm"]
    unsafe extern "C++" {
        type DecryptedBlockImpl;

        fn NewDecryptedBlockImpl() -> UniquePtr<DecryptedBlockImpl>;

        fn Cast(self: Pin<&mut Self>) -> *mut DecryptedBlock;
    }

    #[namespace = "cdm"]
    unsafe extern "C++" {

        type ContentDecryptionModule_10;

        fn Initialize(
            self: Pin<&mut Self>,
            allow_distinctive_identifier: bool,
            allow_persistent_state: bool,
            use_hw_secure_codecs: bool,
        );

        unsafe fn SetServerCertificate(
            self: Pin<&mut Self>,
            promise_id: u32,
            server_certificate_data: *const u8,
            server_certificate_data_size: u32,
        );

        // On response, CDM will call either:
        // - Host::OnResolveNewSessionPromise or Host::OnRejectPromise
        unsafe fn CreateSessionAndGenerateRequest(
            self: Pin<&mut Self>,
            promise_id: u32,
            session_type: SessionType,
            init_data_type: InitDataType,
            init_data: *const u8,
            init_data_size: u32,
        );

        unsafe fn UpdateSession(
            self: Pin<&mut Self>,
            promise_id: u32,
            session_id: *const c_char,
            session_id_size: u32,
            response: *const u8,
            response_size: u32,
        );

        unsafe fn CloseSession(
            self: Pin<&mut Self>,
            promise_id: u32,
            session_id: *const c_char,
            session_id_size: u32,
        );

        // Exposed in the C++ wrapper class above.
        // unsafe fn TimerExpired(self: Pin<&mut Self>, context: *mut c_void);

        unsafe fn Decrypt(
            self: Pin<&mut Self>,
            encrypted_buffer: &InputBuffer_2,
            decrypted_buffer: *mut DecryptedBlock,
        ) -> Status;

        fn OnQueryOutputProtectionStatus(
            self: Pin<&mut Self>,
            result: QueryResult,
            link_mask: u32,
            output_protection_mask: u32,
        );

        // Called in the destructor in the C++ wrapper class.
        // unsafe fn Destroy(self: Pin<&mut Self>);

    }
}
