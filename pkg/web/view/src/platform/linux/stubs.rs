use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::ptr;
use std::sync::OnceLock;

use base_error::*;

extern "C" {
    pub fn malloc(size: usize) -> *mut c_void;
    pub fn free(ptr: *mut c_void);
    pub fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;

    pub fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    pub fn dlclose(handle: *mut c_void) -> c_int;
    pub fn dlerror() -> *const c_char;
}

pub const RTLD_NOW: c_int = 2;
pub const RTLD_GLOBAL: c_int = 256;

pub struct GtkWebkitVtable {
    pub _lib_handle: *mut c_void,
    pub gtk_init: extern "C" fn(*mut c_int, *mut *mut *mut c_char),
    pub gtk_window_new: extern "C" fn(c_int) -> *mut c_void,
    pub gtk_window_set_title: extern "C" fn(*mut c_void, *const c_char),
    pub gtk_window_set_icon: extern "C" fn(*mut c_void, *mut c_void),
    pub gtk_window_set_default_size: extern "C" fn(*mut c_void, c_int, c_int),
    pub gtk_window_resize: extern "C" fn(*mut c_void, c_int, c_int),
    pub gtk_window_maximize: extern "C" fn(*mut c_void),
    pub gtk_container_add: extern "C" fn(*mut c_void, *mut c_void),
    pub gtk_widget_show_all: extern "C" fn(*mut c_void),
    pub gtk_settings_get_default: extern "C" fn() -> *mut c_void,
    pub g_object_set: unsafe extern "C" fn(*mut c_void, *const c_char, ...),
    pub gtk_main: extern "C" fn(),
    pub gtk_main_quit: extern "C" fn(),
    pub g_signal_connect_data: extern "C" fn(*mut c_void, *const c_char, *mut c_void, *mut c_void, *mut c_void, c_int) -> c_uint,
    pub g_idle_add: extern "C" fn(extern "C" fn(*mut c_void) -> c_int, *mut c_void) -> c_uint,
    pub g_free: extern "C" fn(*mut c_void),
    pub g_object_ref: extern "C" fn(*mut c_void) -> *mut c_void,
    pub g_object_unref: extern "C" fn(*mut c_void),
    pub gdk_pixbuf_new_from_data: extern "C" fn(*const u8, c_int, c_int, c_int, c_int, c_int, c_int, *mut c_void, *mut c_void) -> *mut c_void,
    pub g_memory_input_stream_new: extern "C" fn() -> *mut c_void,
    pub g_memory_input_stream_add_data: extern "C" fn(*mut c_void, *mut c_void, isize, Option<extern "C" fn(*mut c_void)>),
    pub webkit_website_data_manager_new: unsafe extern "C" fn(*const c_char, ...) -> *mut c_void,
    pub webkit_web_context_new_with_website_data_manager: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_web_view_new_with_context: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_web_view_new: extern "C" fn() -> *mut c_void,
    pub webkit_web_view_load_html: extern "C" fn(*mut c_void, *const c_char, *const c_char),
    pub webkit_web_view_get_user_content_manager: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_user_content_manager_register_script_message_handler: extern "C" fn(*mut c_void, *const c_char),
    pub webkit_web_view_run_javascript: extern "C" fn(*mut c_void, *const c_char, *mut c_void, *mut c_void, *mut c_void),
    pub webkit_web_view_get_settings: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_settings_set_enable_write_console_messages_to_stdout: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_disable_web_security: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_allow_file_access_from_file_urls: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_allow_universal_access_from_file_urls: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_enable_webgl: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_hardware_acceleration_policy: extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_enable_developer_extras: extern "C" fn(*mut c_void, c_int),
    pub webkit_web_view_get_inspector: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_web_inspector_show: extern "C" fn(*mut c_void),
    pub webkit_web_view_get_context: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_web_context_get_security_manager: extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_security_manager_register_uri_scheme_as_secure: extern "C" fn(*mut c_void, *const c_char),
    pub webkit_security_manager_register_uri_scheme_as_cors_enabled: extern "C" fn(*mut c_void, *const c_char),
    pub webkit_security_manager_register_uri_scheme_as_local: extern "C" fn(*mut c_void, *const c_char),
    pub webkit_web_context_register_uri_scheme: extern "C" fn(*mut c_void, *const c_char, extern "C" fn(*mut c_void, *mut c_void), *mut c_void, *mut c_void),
    pub webkit_uri_scheme_request_get_uri: extern "C" fn(*mut c_void) -> *const c_char,
    pub webkit_uri_scheme_request_finish: extern "C" fn(*mut c_void, *mut c_void, i64, *const c_char),
    pub webkit_javascript_result_get_js_value: extern "C" fn(*mut c_void) -> *mut c_void,
    pub jsc_value_to_string: extern "C" fn(*mut c_void) -> *mut c_char,
    pub gtk_file_chooser_native_new: extern "C" fn(*const c_char, *mut c_void, c_int, *const c_char, *const c_char) -> *mut c_void,
    pub gtk_native_dialog_run: extern "C" fn(*mut c_void) -> c_int,
    pub gtk_file_chooser_get_filename: extern "C" fn(*mut c_void) -> *mut c_char,
    pub gtk_file_chooser_set_current_name: extern "C" fn(*mut c_void, *const c_char),
    pub gtk_native_dialog_destroy: extern "C" fn(*mut c_void),
}

unsafe impl Send for GtkWebkitVtable {}
unsafe impl Sync for GtkWebkitVtable {}

impl GtkWebkitVtable {
    pub unsafe fn load() -> Result<Self> {
        let candidates = [
            b"libwebkit2gtk-4.1.so.0\0".as_ptr() as *const c_char,
            b"libwebkit2gtk-4.1.so\0".as_ptr() as *const c_char,
            b"libwebkit2gtk-4.0.so.37\0".as_ptr() as *const c_char,
            b"libwebkit2gtk-4.0.so\0".as_ptr() as *const c_char,
        ];

        let mut handle = ptr::null_mut();
        for lib in candidates.iter() {
            handle = dlopen(*lib, RTLD_NOW | RTLD_GLOBAL);
            if !handle.is_null() {
                break;
            }
        }
        if handle.is_null() {
            let err = dlerror();
            let err_str = if !err.is_null() {
                CStr::from_ptr(err).to_string_lossy().to_string()
            } else {
                "Unknown dlopen error".to_string()
            };
            return Err(format_err!("Could not dynamically load WebKit2GTK (checked libwebkit2gtk-4.1 and 4.0): {}", err_str));
        }

        macro_rules! load_sym {
            ($name:literal) => {{
                let sym = dlsym(handle, $name.as_ptr() as *const c_char);
                if sym.is_null() {
                    let err = dlerror();
                    let err_str = if !err.is_null() {
                        CStr::from_ptr(err).to_string_lossy().to_string()
                    } else {
                        "Unknown symbol".to_string()
                    };
                    dlclose(handle);
                    return Err(format_err!("Failed to load required symbol '{}' from dynamic library: {}", CStr::from_bytes_with_nul($name).unwrap().to_string_lossy(), err_str));
                }
                std::mem::transmute(sym)
            }};
        }

        Ok(Self {
            _lib_handle: handle,
            gtk_init: load_sym!(b"gtk_init\0"),
            gtk_window_new: load_sym!(b"gtk_window_new\0"),
            gtk_window_set_title: load_sym!(b"gtk_window_set_title\0"),
            gtk_window_set_icon: load_sym!(b"gtk_window_set_icon\0"),
            gtk_window_set_default_size: load_sym!(b"gtk_window_set_default_size\0"),
            gtk_window_resize: load_sym!(b"gtk_window_resize\0"),
            gtk_window_maximize: load_sym!(b"gtk_window_maximize\0"),
            gtk_container_add: load_sym!(b"gtk_container_add\0"),
            gtk_widget_show_all: load_sym!(b"gtk_widget_show_all\0"),
            gtk_settings_get_default: load_sym!(b"gtk_settings_get_default\0"),
            g_object_set: load_sym!(b"g_object_set\0"),
            gtk_main: load_sym!(b"gtk_main\0"),
            gtk_main_quit: load_sym!(b"gtk_main_quit\0"),
            g_signal_connect_data: load_sym!(b"g_signal_connect_data\0"),
            g_idle_add: load_sym!(b"g_idle_add\0"),
            g_free: load_sym!(b"g_free\0"),
            g_object_ref: load_sym!(b"g_object_ref\0"),
            g_object_unref: load_sym!(b"g_object_unref\0"),
            gdk_pixbuf_new_from_data: {
                let gdk_handle = dlopen(b"libgdk_pixbuf-2.0.so.0\0".as_ptr() as *const c_char, RTLD_NOW | RTLD_GLOBAL);
                if gdk_handle.is_null() {
                    let gdk_handle2 = dlopen(b"libgdk_pixbuf-2.0.so\0".as_ptr() as *const c_char, RTLD_NOW | RTLD_GLOBAL);
                    if gdk_handle2.is_null() {
                        return Err(format_err!("Could not load libgdk_pixbuf-2.0"));
                    }
                    let sym = dlsym(gdk_handle2, b"gdk_pixbuf_new_from_data\0".as_ptr() as *const c_char);
                    std::mem::transmute(sym)
                } else {
                    let sym = dlsym(gdk_handle, b"gdk_pixbuf_new_from_data\0".as_ptr() as *const c_char);
                    std::mem::transmute(sym)
                }
            },
            g_memory_input_stream_new: load_sym!(b"g_memory_input_stream_new\0"),
            g_memory_input_stream_add_data: load_sym!(b"g_memory_input_stream_add_data\0"),
            webkit_website_data_manager_new: load_sym!(b"webkit_website_data_manager_new\0"),
            webkit_web_context_new_with_website_data_manager: load_sym!(b"webkit_web_context_new_with_website_data_manager\0"),
            webkit_web_view_new_with_context: load_sym!(b"webkit_web_view_new_with_context\0"),
            webkit_web_view_new: load_sym!(b"webkit_web_view_new\0"),
            webkit_web_view_load_html: load_sym!(b"webkit_web_view_load_html\0"),
            webkit_web_view_get_user_content_manager: load_sym!(b"webkit_web_view_get_user_content_manager\0"),
            webkit_user_content_manager_register_script_message_handler: load_sym!(b"webkit_user_content_manager_register_script_message_handler\0"),
            webkit_web_view_run_javascript: load_sym!(b"webkit_web_view_run_javascript\0"),
            webkit_web_view_get_settings: load_sym!(b"webkit_web_view_get_settings\0"),
            webkit_settings_set_enable_write_console_messages_to_stdout: load_sym!(b"webkit_settings_set_enable_write_console_messages_to_stdout\0"),
            webkit_settings_set_disable_web_security: load_sym!(b"webkit_settings_set_disable_web_security\0"),
            webkit_settings_set_allow_file_access_from_file_urls: load_sym!(b"webkit_settings_set_allow_file_access_from_file_urls\0"),
            webkit_settings_set_allow_universal_access_from_file_urls: load_sym!(b"webkit_settings_set_allow_universal_access_from_file_urls\0"),
            webkit_settings_set_enable_webgl: load_sym!(b"webkit_settings_set_enable_webgl\0"),
            webkit_settings_set_hardware_acceleration_policy: load_sym!(b"webkit_settings_set_hardware_acceleration_policy\0"),
            webkit_settings_set_enable_developer_extras: load_sym!(b"webkit_settings_set_enable_developer_extras\0"),
            webkit_web_view_get_inspector: load_sym!(b"webkit_web_view_get_inspector\0"),
            webkit_web_inspector_show: load_sym!(b"webkit_web_inspector_show\0"),
            webkit_web_view_get_context: load_sym!(b"webkit_web_view_get_context\0"),
            webkit_web_context_get_security_manager: load_sym!(b"webkit_web_context_get_security_manager\0"),
            webkit_security_manager_register_uri_scheme_as_secure: load_sym!(b"webkit_security_manager_register_uri_scheme_as_secure\0"),
            webkit_security_manager_register_uri_scheme_as_cors_enabled: load_sym!(b"webkit_security_manager_register_uri_scheme_as_cors_enabled\0"),
            webkit_security_manager_register_uri_scheme_as_local: load_sym!(b"webkit_security_manager_register_uri_scheme_as_local\0"),
            webkit_web_context_register_uri_scheme: load_sym!(b"webkit_web_context_register_uri_scheme\0"),
            webkit_uri_scheme_request_get_uri: load_sym!(b"webkit_uri_scheme_request_get_uri\0"),
            webkit_uri_scheme_request_finish: load_sym!(b"webkit_uri_scheme_request_finish\0"),
            webkit_javascript_result_get_js_value: load_sym!(b"webkit_javascript_result_get_js_value\0"),
            jsc_value_to_string: load_sym!(b"jsc_value_to_string\0"),
            gtk_file_chooser_native_new: load_sym!(b"gtk_file_chooser_native_new\0"),
            gtk_native_dialog_run: load_sym!(b"gtk_native_dialog_run\0"),
            gtk_file_chooser_get_filename: load_sym!(b"gtk_file_chooser_get_filename\0"),
            gtk_file_chooser_set_current_name: load_sym!(b"gtk_file_chooser_set_current_name\0"),
            gtk_native_dialog_destroy: load_sym!(b"gtk_native_dialog_destroy\0"),
        })
    }
}

static VTABLE: OnceLock<GtkWebkitVtable> = OnceLock::new();

pub fn vtable() -> Result<&'static GtkWebkitVtable> {
    if let Some(vt) = VTABLE.get() {
        return Ok(vt);
    }
    let loaded = unsafe { GtkWebkitVtable::load()? };
    let _ = VTABLE.set(loaded);
    VTABLE.get().ok_or_else(|| err_msg("Failed to retrieve initialized VTABLE"))
}

#[inline(always)]
pub fn get_vtable() -> &'static GtkWebkitVtable {
    VTABLE.get().expect("Runtime VTABLE not initialized before FFI usage")
}

#[inline(always)]
pub unsafe fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char) { (get_vtable().gtk_init)(argc, argv) }
#[inline(always)]
pub unsafe fn gtk_window_new(window_type: c_int) -> *mut c_void { (get_vtable().gtk_window_new)(window_type) }
#[inline(always)]
#[inline(always)]
pub unsafe fn gtk_window_set_title(window: *mut c_void, title: *const c_char) { (get_vtable().gtk_window_set_title)(window, title) }
#[inline(always)]
pub unsafe fn gtk_window_set_icon(window: *mut c_void, icon: *mut c_void) { (get_vtable().gtk_window_set_icon)(window, icon) }
#[inline(always)]
pub unsafe fn gtk_window_set_default_size(window: *mut c_void, width: c_int, height: c_int) { (get_vtable().gtk_window_set_default_size)(window, width, height) }
#[inline(always)]
pub unsafe fn gtk_window_resize(window: *mut c_void, width: c_int, height: c_int) { (get_vtable().gtk_window_resize)(window, width, height) }
#[inline(always)]
pub unsafe fn gtk_window_maximize(window: *mut c_void) { (get_vtable().gtk_window_maximize)(window) }
#[inline(always)]
pub unsafe fn gtk_container_add(container: *mut c_void, widget: *mut c_void) { (get_vtable().gtk_container_add)(container, widget) }
#[inline(always)]
pub unsafe fn gtk_widget_show_all(widget: *mut c_void) { (get_vtable().gtk_widget_show_all)(widget) }
#[inline(always)]
pub unsafe fn gtk_settings_get_default() -> *mut c_void { (get_vtable().gtk_settings_get_default)() }
// g_object_set is varargs, invoked directly from vtable using unsafe extern "C" fn in mod.rs
#[inline(always)]
pub unsafe fn gtk_main() { (get_vtable().gtk_main)() }
#[inline(always)]
pub unsafe fn gtk_main_quit() { (get_vtable().gtk_main_quit)() }
#[inline(always)]
pub unsafe fn g_signal_connect_data(instance: *mut c_void, detailed_signal: *const c_char, c_handler: *mut c_void, data: *mut c_void, destroy_data: *mut c_void, connect_flags: c_int) -> c_uint { (get_vtable().g_signal_connect_data)(instance, detailed_signal, c_handler, data, destroy_data, connect_flags) }
#[inline(always)]
pub unsafe fn g_idle_add(function: extern "C" fn(*mut c_void) -> c_int, data: *mut c_void) -> c_uint { (get_vtable().g_idle_add)(function, data) }
#[inline(always)]
pub unsafe fn g_free(ptr: *mut c_void) { (get_vtable().g_free)(ptr) }
#[inline(always)]
#[inline(always)]
pub unsafe fn g_object_ref(object: *mut c_void) -> *mut c_void { (get_vtable().g_object_ref)(object) }
#[inline(always)]
pub unsafe fn g_object_unref(object: *mut c_void) { (get_vtable().g_object_unref)(object) }
#[inline(always)]
pub unsafe fn gdk_pixbuf_new_from_data(data: *const u8, colorspace: c_int, has_alpha: c_int, bits_per_sample: c_int, width: c_int, height: c_int, rowstride: c_int, destroy_fn: *mut c_void, destroy_fn_data: *mut c_void) -> *mut c_void { (get_vtable().gdk_pixbuf_new_from_data)(data, colorspace, has_alpha, bits_per_sample, width, height, rowstride, destroy_fn, destroy_fn_data) }
#[inline(always)]
pub unsafe fn g_memory_input_stream_new() -> *mut c_void { (get_vtable().g_memory_input_stream_new)() }
#[inline(always)]
pub unsafe fn g_memory_input_stream_add_data(stream: *mut c_void, data: *mut c_void, len: isize, destroy: Option<extern "C" fn(*mut c_void)>) { (get_vtable().g_memory_input_stream_add_data)(stream, data, len, destroy) }
// webkit_website_data_manager_new is varargs, invoked directly from vtable
#[inline(always)]
pub unsafe fn webkit_web_context_new_with_website_data_manager(manager: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_context_new_with_website_data_manager)(manager) }
#[inline(always)]
pub unsafe fn webkit_web_view_new_with_context(context: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_view_new_with_context)(context) }
#[inline(always)]
pub unsafe fn webkit_web_view_new() -> *mut c_void { (get_vtable().webkit_web_view_new)() }
#[inline(always)]
pub unsafe fn webkit_web_view_load_html(web_view: *mut c_void, content: *const c_char, base_uri: *const c_char) { (get_vtable().webkit_web_view_load_html)(web_view, content, base_uri) }
#[inline(always)]
pub unsafe fn webkit_web_view_get_user_content_manager(web_view: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_view_get_user_content_manager)(web_view) }
#[inline(always)]
pub unsafe fn webkit_user_content_manager_register_script_message_handler(ucm: *mut c_void, name: *const c_char) { (get_vtable().webkit_user_content_manager_register_script_message_handler)(ucm, name) }
#[inline(always)]
pub unsafe fn webkit_web_view_run_javascript(web_view: *mut c_void, script: *const c_char, cancellable: *mut c_void, callback: *mut c_void, user_data: *mut c_void) { (get_vtable().webkit_web_view_run_javascript)(web_view, script, cancellable, callback, user_data) }
#[inline(always)]
pub unsafe fn webkit_web_view_get_settings(web_view: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_view_get_settings)(web_view) }
#[inline(always)]
pub unsafe fn webkit_settings_set_enable_write_console_messages_to_stdout(settings: *mut c_void, enabled: c_int) { (get_vtable().webkit_settings_set_enable_write_console_messages_to_stdout)(settings, enabled) }
#[inline(always)]
pub unsafe fn webkit_settings_set_disable_web_security(settings: *mut c_void, disabled: c_int) { (get_vtable().webkit_settings_set_disable_web_security)(settings, disabled) }
#[inline(always)]
pub unsafe fn webkit_settings_set_allow_file_access_from_file_urls(settings: *mut c_void, allowed: c_int) { (get_vtable().webkit_settings_set_allow_file_access_from_file_urls)(settings, allowed) }
#[inline(always)]
pub unsafe fn webkit_settings_set_allow_universal_access_from_file_urls(settings: *mut c_void, allowed: c_int) { (get_vtable().webkit_settings_set_allow_universal_access_from_file_urls)(settings, allowed) }
#[inline(always)]
pub unsafe fn webkit_settings_set_enable_webgl(settings: *mut c_void, enabled: c_int) { (get_vtable().webkit_settings_set_enable_webgl)(settings, enabled) }
#[inline(always)]
pub unsafe fn webkit_settings_set_hardware_acceleration_policy(settings: *mut c_void, policy: c_int) { (get_vtable().webkit_settings_set_hardware_acceleration_policy)(settings, policy) }
#[inline(always)]
pub unsafe fn webkit_settings_set_enable_developer_extras(settings: *mut c_void, enabled: c_int) { (get_vtable().webkit_settings_set_enable_developer_extras)(settings, enabled) }
#[inline(always)]
pub unsafe fn webkit_web_view_get_inspector(web_view: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_view_get_inspector)(web_view) }
#[inline(always)]
pub unsafe fn webkit_web_inspector_show(inspector: *mut c_void) { (get_vtable().webkit_web_inspector_show)(inspector) }
#[inline(always)]
pub unsafe fn webkit_web_view_get_context(web_view: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_view_get_context)(web_view) }
#[inline(always)]
pub unsafe fn webkit_web_context_get_security_manager(context: *mut c_void) -> *mut c_void { (get_vtable().webkit_web_context_get_security_manager)(context) }
#[inline(always)]
pub unsafe fn webkit_security_manager_register_uri_scheme_as_secure(sm: *mut c_void, scheme: *const c_char) { (get_vtable().webkit_security_manager_register_uri_scheme_as_secure)(sm, scheme) }
#[inline(always)]
pub unsafe fn webkit_security_manager_register_uri_scheme_as_cors_enabled(sm: *mut c_void, scheme: *const c_char) { (get_vtable().webkit_security_manager_register_uri_scheme_as_cors_enabled)(sm, scheme) }
#[inline(always)]
pub unsafe fn webkit_security_manager_register_uri_scheme_as_local(sm: *mut c_void, scheme: *const c_char) { (get_vtable().webkit_security_manager_register_uri_scheme_as_local)(sm, scheme) }
#[inline(always)]
pub unsafe fn webkit_web_context_register_uri_scheme(context: *mut c_void, scheme: *const c_char, callback: extern "C" fn(*mut c_void, *mut c_void), user_data: *mut c_void, destroy_notify: *mut c_void) { (get_vtable().webkit_web_context_register_uri_scheme)(context, scheme, callback, user_data, destroy_notify) }
#[inline(always)]
pub unsafe fn webkit_uri_scheme_request_get_uri(request: *mut c_void) -> *const c_char { (get_vtable().webkit_uri_scheme_request_get_uri)(request) }
#[inline(always)]
pub unsafe fn webkit_uri_scheme_request_finish(request: *mut c_void, stream: *mut c_void, stream_length: i64, content_type: *const c_char) { (get_vtable().webkit_uri_scheme_request_finish)(request, stream, stream_length, content_type) }
#[inline(always)]
pub unsafe fn webkit_javascript_result_get_js_value(js_result: *mut c_void) -> *mut c_void { (get_vtable().webkit_javascript_result_get_js_value)(js_result) }
#[inline(always)]
pub unsafe fn jsc_value_to_string(value: *mut c_void) -> *mut c_char { (get_vtable().jsc_value_to_string)(value) }
#[inline(always)]
pub unsafe fn gtk_file_chooser_native_new(title: *const c_char, parent: *mut c_void, action: c_int, accept: *const c_char, cancel: *const c_char) -> *mut c_void { (get_vtable().gtk_file_chooser_native_new)(title, parent, action, accept, cancel) }
#[inline(always)]
pub unsafe fn gtk_native_dialog_run(dialog: *mut c_void) -> c_int { (get_vtable().gtk_native_dialog_run)(dialog) }
#[inline(always)]
pub unsafe fn gtk_file_chooser_get_filename(chooser: *mut c_void) -> *mut c_char { (get_vtable().gtk_file_chooser_get_filename)(chooser) }
#[inline(always)]
pub unsafe fn gtk_file_chooser_set_current_name(chooser: *mut c_void, name: *const c_char) { (get_vtable().gtk_file_chooser_set_current_name)(chooser, name) }
#[inline(always)]
pub unsafe fn gtk_native_dialog_destroy(dialog: *mut c_void) { (get_vtable().gtk_native_dialog_destroy)(dialog) }

