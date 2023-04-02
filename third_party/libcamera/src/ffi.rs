use std::pin::Pin;

use cxx::{type_id, ExternType};

use crate::bindings;

pub use self::ffi::*;

pub struct RequestCompleteContext {
    pub handler: Box<dyn Fn(&Request) + Send + Sync + 'static>,
}

// NOTE: Keep in sync with the 'using' definitions in ffi.rs.

unsafe impl ExternType for bindings::StreamRole {
    type Id = type_id!("libcamera::StreamRole");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::Request_Status {
    type Id = type_id!("libcamera::RequestStatus");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::Request_ReuseFlag {
    type Id = type_id!("libcamera::RequestReuseFlag");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::FrameMetadata_Status {
    type Id = type_id!("libcamera::FrameStatus");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::FrameMetadata_Plane {
    type Id = type_id!("libcamera::FramePlaneMetadata");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::CameraConfiguration_Status {
    type Id = type_id!("libcamera::CameraConfigurationStatus");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::PixelFormat {
    type Id = type_id!("libcamera::PixelFormat");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::Size {
    type Id = type_id!("libcamera::Size");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::SizeRange {
    type Id = type_id!("libcamera::SizeRange");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::ControlType {
    type Id = type_id!("libcamera::ControlType");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::Rectangle {
    type Id = type_id!("libcamera::Rectangle");
    type Kind = cxx::kind::Trivial;
}

unsafe impl ExternType for bindings::ColorSpace {
    type Id = type_id!("libcamera::ColorSpace");
    type Kind = cxx::kind::Trivial;
}

#[cxx::bridge]
mod ffi {
    /// A mirror of libcamera::FrameBuffer::Plane
    #[derive(Debug, Clone, Copy)]
    struct FrameBufferPlane {
        fd: u32,
        offset: u32,
        length: u32,
    }

    #[derive(Debug, Clone)]
    struct FrameMetadata {
        status: FrameStatus,
        sequence: u32,
        timestamp: u64,
        planes: Vec<FramePlaneMetadataWrap>,
    }

    // TODO: Standard on 'XShared' nameing here?

    // Wrapper to work around https://github.com/dtolnay/cxx/issues/741
    struct CameraPtr {
        camera: SharedPtr<Camera>,
    }

    struct FrameBufferPtr {
        buffer: *mut FrameBuffer,
    }

    #[derive(Debug, Clone)]
    struct FramePlaneMetadataWrap {
        inner: FramePlaneMetadata,
    }

    struct StreamPtr {
        stream: *mut Stream,
    }

    struct PixelFormatWrap {
        value: PixelFormat,
    }

    struct SizeWrap {
        value: Size,
    }

    struct ControlInfoMapEntry<'a> {
        key: &'a ControlId,
        value: &'a ControlInfo,
    }

    struct ControlListEntry<'a> {
        key: u32,
        value: &'a ControlValue,
    }

    extern "Rust" {
        type RequestCompleteContext;
    }

    #[namespace = "libcamera"]
    unsafe extern "C++" {
        include!("libcamera/src/ffi.h");

        // NOTE: Keep in sync with ExternType definitions above.
        type StreamRole = crate::bindings::StreamRole;
        type RequestStatus = crate::bindings::Request_Status;
        type RequestReuseFlag = crate::bindings::Request_ReuseFlag;
        type FrameStatus = crate::bindings::FrameMetadata_Status;
        type FramePlaneMetadata = crate::bindings::FrameMetadata_Plane;
        type CameraConfigurationStatus = crate::bindings::CameraConfiguration_Status;
        type PixelFormat = crate::bindings::PixelFormat;
        type Size = crate::bindings::Size;
        type SizeRange = crate::bindings::SizeRange;
        type ControlType = crate::bindings::ControlType;
        type Rectangle = crate::bindings::Rectangle;
        type ColorSpace = crate::bindings::ColorSpace;

        //////////////////////////////////////

        type CameraManager;

        fn new_camera_manager() -> UniquePtr<CameraManager>;

        fn start(self: Pin<&mut CameraManager>) -> i32;

        fn stop(self: Pin<&mut CameraManager>);

        fn list_cameras(camera_manager: &CameraManager) -> Vec<CameraPtr>;

        //////////////////////////////////////

        type Camera;

        fn id(self: &Camera) -> &CxxString;

        /// Thread safe
        fn acquire(self: Pin<&mut Camera>) -> i32;

        fn release(self: Pin<&mut Camera>) -> i32;

        // Thread safe
        fn generate_camera_configuration(
            camera: Pin<&mut Camera>,
            stream_roles: &[StreamRole],
        ) -> UniquePtr<CameraConfiguration>;

        unsafe fn start(self: Pin<&mut Camera>, control_list: *const ControlList) -> i32;

        fn stop(self: Pin<&mut Camera>) -> i32;

        unsafe fn configure(self: Pin<&mut Camera>, config: *mut CameraConfiguration) -> i32;

        /// Thread safe
        fn createRequest(self: Pin<&mut Camera>, cookie: u64) -> UniquePtr<Request>;

        /// Thread safe
        unsafe fn queueRequest(self: Pin<&mut Camera>, request: *mut Request) -> i32;

        fn properties(self: &Camera) -> &ControlList;

        fn controls(self: &Camera) -> &ControlInfoMap;

        /// Thread safe
        type RequestCompleteSlot;
        fn camera_connect_request_completed(
            camera: Pin<&mut Camera>,
            handler: fn(&RequestCompleteContext, &Request),
            context: Box<RequestCompleteContext>,
        ) -> UniquePtr<RequestCompleteSlot>;

        fn camera_streams(camera: &Camera) -> Vec<StreamPtr>;

        unsafe fn camera_contains_stream(camera: &Camera, stream: *mut Stream) -> bool;

        //////////////////////////////////////

        type Request;

        /// NOTE: Only valid after a request has been completed.
        fn sequence(self: &Request) -> u32;

        fn cookie(self: &Request) -> u64;

        // May return a negative error.
        unsafe fn addBuffer(
            self: Pin<&mut Request>,
            stream: *const Stream,
            buffer: *mut FrameBuffer,
            fence: UniquePtr<Fence>,
        ) -> i32;

        fn status(self: &Request) -> RequestStatus;

        fn reuse(self: Pin<&mut Request>, flags: RequestReuseFlag);

        fn controls(self: Pin<&mut Request>) -> Pin<&mut ControlList>;

        fn metadata(self: Pin<&mut Request>) -> Pin<&mut ControlList>;

        fn request_to_string(request: &Request) -> String;

        fn hasPendingBuffers(self: &Request) -> bool;

        //////////////////////////////////////

        type Fence;

        //////////////////////////////////////

        type CameraConfiguration;

        fn at(self: &CameraConfiguration, index: u32) -> &StreamConfiguration;

        #[rust_name = "at_mut"]
        fn at(self: Pin<&mut CameraConfiguration>, index: u32) -> Pin<&mut StreamConfiguration>;

        fn size(self: &CameraConfiguration) -> usize;

        fn validate(self: Pin<&mut CameraConfiguration>) -> CameraConfigurationStatus;

        //////////////////////////////////////

        type StreamConfiguration;

        fn stream_config_pixel_format(config: &StreamConfiguration) -> PixelFormat;
        fn stream_config_set_pixel_format(
            config: Pin<&mut StreamConfiguration>,
            value: PixelFormat,
        );

        fn stream_config_size(config: &StreamConfiguration) -> Size;
        fn stream_config_set_size(config: Pin<&mut StreamConfiguration>, value: Size);

        fn stream_config_stride(config: &StreamConfiguration) -> u32;
        fn stream_config_set_stride(config: Pin<&mut StreamConfiguration>, value: u32);

        fn stream_config_frame_size(config: &StreamConfiguration) -> u32;
        fn stream_config_set_frame_size(config: Pin<&mut StreamConfiguration>, value: u32);

        fn stream_config_buffer_count(config: &StreamConfiguration) -> u32;
        fn stream_config_set_buffer_count(config: Pin<&mut StreamConfiguration>, value: u32);

        fn stream_config_to_string(config: &StreamConfiguration) -> String;

        fn stream(self: &StreamConfiguration) -> *mut Stream;

        fn formats(self: &StreamConfiguration) -> &StreamFormats;

        fn stream_config_has_color_space(config: &StreamConfiguration) -> bool;
        fn stream_config_color_space(config: &StreamConfiguration) -> ColorSpace;
        fn stream_config_set_color_space(config: Pin<&mut StreamConfiguration>, value: ColorSpace);
        fn stream_config_clear_color_space(config: Pin<&mut StreamConfiguration>);

        //////////////////////////////////////

        type Stream;

        //////////////////////////////////////

        fn pixel_format_to_string(format: &PixelFormat) -> String;

        //////////////////////////////////////

        type StreamFormats;

        fn stream_formats_pixelformats(stream_formats: &StreamFormats) -> Vec<PixelFormatWrap>;

        fn stream_formats_sizes(
            stream_formats: &StreamFormats,
            pixelformat: &PixelFormat,
        ) -> Vec<SizeWrap>;

        fn range(self: &StreamFormats, pixelformat: &PixelFormat) -> SizeRange;

        //////////////////////////////////////

        type ControlValue;

        fn new_control_value() -> UniquePtr<ControlValue>;

        #[cxx_name = "type"]
        fn typ(self: &ControlValue) -> ControlType;

        // Redundnat with checking the type.
        // fn isNone(self: &ControlValue) -> bool;

        #[rust_name = "is_array"]
        fn isArray(self: &ControlValue) -> bool;

        #[rust_name = "num_elements"]
        fn numElements(self: &ControlValue) -> usize;

        #[rust_name = "get_bool"]
        fn get(self: &ControlValue) -> bool;

        #[rust_name = "set_bool"]
        fn set(self: Pin<&mut ControlValue>, value: &bool);

        #[rust_name = "control_value_get_bool_array"]
        fn control_value_get_array(value: &ControlValue) -> &[bool];

        #[rust_name = "control_value_set_bool_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[bool]);

        #[rust_name = "get_byte"]
        fn get(self: &ControlValue) -> u8;

        #[rust_name = "set_byte"]
        fn set(self: Pin<&mut ControlValue>, value: &u8);

        #[rust_name = "control_value_get_byte_array"]
        fn control_value_get_array(value: &ControlValue) -> &[u8];

        #[rust_name = "control_value_set_byte_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[u8]);

        #[rust_name = "get_i32"]
        fn get(self: &ControlValue) -> i32;

        #[rust_name = "set_i32"]
        fn set(self: Pin<&mut ControlValue>, value: &i32);

        #[rust_name = "control_value_get_i32_array"]
        fn control_value_get_array(value: &ControlValue) -> &[i32];

        #[rust_name = "control_value_set_i32_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[i32]);

        #[rust_name = "get_i64"]
        fn get(self: &ControlValue) -> i64;

        #[rust_name = "set_i64"]
        fn set(self: Pin<&mut ControlValue>, value: &i64);

        #[rust_name = "control_value_get_i64_array"]
        fn control_value_get_array(value: &ControlValue) -> &[i64];

        #[rust_name = "control_value_set_i64_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[i64]);

        #[rust_name = "get_float"]
        fn get(self: &ControlValue) -> f32;

        #[rust_name = "set_float"]
        fn set(self: Pin<&mut ControlValue>, value: &f32);

        #[rust_name = "control_value_get_float_array"]
        fn control_value_get_array(value: &ControlValue) -> &[f32];

        #[rust_name = "control_value_set_float_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[f32]);

        #[rust_name = "get_rectangle"]
        fn get(self: &ControlValue) -> Rectangle;

        #[rust_name = "set_rectangle"]
        fn set(self: Pin<&mut ControlValue>, value: &Rectangle);

        #[rust_name = "control_value_get_rectangle_array"]
        fn control_value_get_array(value: &ControlValue) -> &[Rectangle];

        #[rust_name = "control_value_set_rectangle_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[Rectangle]);

        #[rust_name = "get_size"]
        fn get(self: &ControlValue) -> Size;

        #[rust_name = "set_size"]
        fn set(self: Pin<&mut ControlValue>, value: &Size);

        #[rust_name = "control_value_get_size_array"]
        fn control_value_get_array(value: &ControlValue) -> &[Size];

        #[rust_name = "control_value_set_size_array"]
        fn control_value_set_array(value: Pin<&mut ControlValue>, array: &[Size]);

        fn control_value_get_string(value: &ControlValue) -> String;

        fn control_value_set_string(value: Pin<&mut ControlValue>, s: &String);

        fn control_value_get_string_array(value: &ControlValue) -> Vec<String>;

        fn control_value_to_string(value: &ControlValue) -> String;

        //////////////////////////////////////

        type ControlList;

        fn new_control_list() -> UniquePtr<ControlList>;

        fn contains(self: &ControlList, id: u32) -> bool;

        fn get(self: &ControlList, id: u32) -> &ControlValue;

        fn set(self: Pin<&mut ControlList>, id: u32, value: &ControlValue);

        fn control_list_entries(list: &ControlList) -> Vec<ControlListEntry>;

        fn idMap(self: &ControlList) -> *const ControlIdMap;

        fn infoMap(self: &ControlList) -> *const ControlInfoMap;

        //////////////////////////////////////

        type ControlId;

        fn id(self: &ControlId) -> u32;

        fn name(self: &ControlId) -> &CxxString;

        #[cxx_name = "type"]
        fn typ(self: &ControlId) -> ControlType;

        //////////////////////////////////////

        type ControlIdMap;

        fn at<'a>(self: &'a ControlIdMap, id: &u32) -> &'a *const ControlId;

        fn contains(self: &ControlIdMap, id: &u32) -> bool;

        //////////////////////////////////////

        type ControlInfoMap;

        fn at<'a>(self: &'a ControlInfoMap, key: u32) -> &'a ControlInfo;

        fn count(self: &ControlInfoMap, key: u32) -> usize;

        fn idmap(self: &ControlInfoMap) -> &ControlIdMap;

        fn control_info_map_entries(map: &ControlInfoMap) -> Vec<ControlInfoMapEntry>;

        //////////////////////////////////////

        type ControlInfo;

        fn min(self: &ControlInfo) -> &ControlValue;

        fn max(self: &ControlInfo) -> &ControlValue;

        fn def(self: &ControlInfo) -> &ControlValue;

        #[rust_name = "values_raw"]
        fn values(self: &ControlInfo) -> &CxxVector<ControlValue>;

        fn control_info_to_string(info: &ControlInfo) -> String;

        //////////////////////////////////////

        type FrameBuffer;

        fn frame_buffer_planes(buffer: &FrameBuffer) -> Vec<FrameBufferPlane>;

        fn frame_buffer_metadata(buffer: &FrameBuffer) -> FrameMetadata;

        //////////////////////////////////////

        type FrameBufferAllocator;

        fn new_frame_buffer_allocator(camera: SharedPtr<Camera>)
            -> UniquePtr<FrameBufferAllocator>;

        unsafe fn allocate(self: Pin<&mut FrameBufferAllocator>, stream: *mut Stream) -> i32;

        unsafe fn free(self: Pin<&mut FrameBufferAllocator>, stream: *mut Stream) -> i32;

        unsafe fn get_allocated_frame_buffers(
            allocator: &FrameBufferAllocator,
            stream: *mut Stream,
        ) -> Vec<FrameBufferPtr>;

        //////////////////////////////////////

    }
}

// Turn the instance method wrapper functions back into instance methods.
impl ControlValue {
    pub fn get_string(&self) -> String {
        ffi::control_value_get_string(self)
    }

    pub fn set_string(self: Pin<&mut Self>, s: &String) {
        ffi::control_value_set_string(self, s);
    }
}
