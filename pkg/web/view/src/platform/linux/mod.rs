pub mod stubs;
use stubs::*;

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicPtr, Ordering};

use base_error::*;
use common::hash::FastHasherBuilder;

use crate::{MessageHandler, RequestHandler, WebViewBuilder, WebViewHandle};

extern "C" fn free_cb(ptr: *mut c_void) {
    unsafe {
        free(ptr);
    }
}

pub(crate) struct LinuxProxyInner {
    pub(crate) web_view: AtomicPtr<c_void>,
    pub(crate) window: AtomicPtr<c_void>,
    pub(crate) main_thread: std::thread::ThreadId,
    pub(crate) requests: Mutex<HashMap<String, usize, FastHasherBuilder>>,
    pub(crate) request_counter: std::sync::atomic::AtomicUsize,
}

impl LinuxProxyInner {
    fn run_open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        unsafe {
            let title_c = CString::new(title)?;
            let window = self.window.load(Ordering::SeqCst);
            let dialog = gtk_file_chooser_native_new(
                title_c.as_ptr(),
                window,
                0, // GTK_FILE_CHOOSER_ACTION_OPEN
                ptr::null(),
                ptr::null(),
            );
            if dialog.is_null() {
                return Err(err_msg("Failed to create GtkFileChooserNative dialog"));
            }
            let res = gtk_native_dialog_run(dialog);
            let result = if res == -3 { // GTK_RESPONSE_ACCEPT
                let ptr = gtk_file_chooser_get_filename(dialog);
                if !ptr.is_null() {
                    let s = CStr::from_ptr(ptr).to_string_lossy().to_string();
                    g_free(ptr as *mut _);
                    Ok(Some(s))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            };
            gtk_native_dialog_destroy(dialog);
            result
        }
    }

    fn run_save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        unsafe {
            let title_c = CString::new(title)?;
            let window = self.window.load(Ordering::SeqCst);
            let dialog = gtk_file_chooser_native_new(
                title_c.as_ptr(),
                window,
                1, // GTK_FILE_CHOOSER_ACTION_SAVE
                ptr::null(),
                ptr::null(),
            );
            if dialog.is_null() {
                return Err(err_msg("Failed to create GtkFileChooserNative dialog"));
            }
            if let Some(name) = default_name {
                if let Ok(name_c) = CString::new(name) {
                    gtk_file_chooser_set_current_name(dialog, name_c.as_ptr());
                }
            }
            let res = gtk_native_dialog_run(dialog);
            let result = if res == -3 { // GTK_RESPONSE_ACCEPT
                let ptr = gtk_file_chooser_get_filename(dialog);
                if !ptr.is_null() {
                    let s = CStr::from_ptr(ptr).to_string_lossy().to_string();
                    g_free(ptr as *mut _);
                    Ok(Some(s))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            };
            gtk_native_dialog_destroy(dialog);
            result
        }
    }
}

#[derive(Clone)]
pub struct WebViewProxy {
    pub(crate) inner: Arc<LinuxProxyInner>,
}

enum GuiTask {
    EvalJs(String),
    SendResponse { request_id: String, status_code: u16, mime_type: String, body: Vec<u8> },
    AbortRequest { request_id: String },
    OpenFileDialog {
        title: String,
        sender: std::sync::mpsc::Sender<Result<Option<String>>>,
    },
    SaveFileDialog {
        title: String,
        default_name: Option<String>,
        sender: std::sync::mpsc::Sender<Result<Option<String>>>,
    },
}

extern "C" fn idle_task_cb(data: *mut c_void) -> c_int {
    unsafe {
        let box_data = Box::from_raw(data as *mut (Arc<LinuxProxyInner>, GuiTask));
        let (inner, task) = *box_data;
        let web_view = inner.web_view.load(Ordering::SeqCst);

        match task {
            GuiTask::EvalJs(script) => {
                if !web_view.is_null() {
                    if let Ok(c_script) = CString::new(script) {
                        webkit_web_view_run_javascript(
                            web_view,
                            c_script.as_ptr(),
                            ptr::null_mut(),
                            ptr::null_mut(),
                            ptr::null_mut(),
                        );
                    }
                }
            }
            GuiTask::SendResponse { request_id, status_code: _, mime_type, body } => {
                let req_ptr = {
                    let mut requests = inner.requests.lock().unwrap();
                    requests.remove(&request_id)
                };
                if let Some(req_val) = req_ptr {
                    let request = req_val as *mut c_void;
                    let len = body.len();
                    let stream = g_memory_input_stream_new();
                    if len > 0 {
                        let ptr = malloc(len);
                        memcpy(ptr, body.as_ptr() as *const c_void, len);
                        g_memory_input_stream_add_data(
                            stream,
                            ptr,
                            len as isize,
                            Some(free_cb),
                        );
                    }
                    // Status code is intentionally ignored in legacy WebKit C-API because it lacks a response object
                    let mime_cstr = CString::new(mime_type).unwrap_or_else(|_| CString::new("application/octet-stream").unwrap());
                    webkit_uri_scheme_request_finish(request, stream, len as i64, mime_cstr.as_ptr());
                }
            }
            GuiTask::AbortRequest { request_id } => {
                let req_ptr = {
                    let mut requests = inner.requests.lock().unwrap();
                    requests.remove(&request_id)
                };
                if let Some(req_val) = req_ptr {
                    let request = req_val as *mut c_void;
                    let stream = g_memory_input_stream_new();
                    let mime_cstr = CString::new("text/plain").unwrap();
                    // Finish with empty stream to simulate abort/empty
                    webkit_uri_scheme_request_finish(request, stream, 0, mime_cstr.as_ptr());
                }
            }
            GuiTask::OpenFileDialog { title, sender } => {
                let _ = sender.send(inner.run_open_file_dialog(&title));
            }
            GuiTask::SaveFileDialog { title, default_name, sender } => {
                let _ = sender.send(inner.run_save_file_dialog(&title, default_name.as_deref()));
            }
        }
        0 // G_SOURCE_REMOVE (execute once)
    }
}

impl WebViewProxy {
    pub fn post_message(&self, message: &str) -> Result<()> {
        self.eval_js(&super::create_post_message_script(message))
    }

    fn post_gui_task(&self, task: GuiTask) -> Result<()> {
        let box_task = Box::new((self.inner.clone(), task));
        unsafe {
            g_idle_add(idle_task_cb, Box::into_raw(box_task) as *mut c_void);
        }
        Ok(())
    }

    pub fn eval_js(&self, script: &str) -> Result<()> {
        self.post_gui_task(GuiTask::EvalJs(script.to_string()))
    }

    pub fn send_binary(&self, _stream_id: &str, _data: &[u8]) -> Result<()> {
        Err(err_msg("Binary push streaming is only supported on Windows. Use send_response instead."))
    }

    pub fn send_response(&self, request_id: &str, status_code: u16, mime_type: &str, body: &[u8]) -> Result<()> {
        let task = GuiTask::SendResponse {
            request_id: request_id.to_string(),
            status_code,
            mime_type: mime_type.to_string(),
            body: body.to_vec(),
        };
        self.post_gui_task(task)
    }

    pub fn abort_request(&self, request_id: &str) -> Result<()> {
        let task = GuiTask::AbortRequest {
            request_id: request_id.to_string(),
        };
        self.post_gui_task(task)
    }

    pub fn open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        if std::thread::current().id() == self.inner.main_thread {
            self.inner.run_open_file_dialog(title)
        } else {
            let (tx, rx) = std::sync::mpsc::channel();
            let task = Box::new((
                self.inner.clone(),
                GuiTask::OpenFileDialog {
                    title: title.to_string(),
                    sender: tx,
                },
            ));
            unsafe {
                g_idle_add(idle_task_cb, Box::into_raw(task) as *mut c_void);
            }
            rx.recv()?
        }
    }

    pub fn save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        if std::thread::current().id() == self.inner.main_thread {
            self.inner.run_save_file_dialog(title, default_name)
        } else {
            let (tx, rx) = std::sync::mpsc::channel();
            let task = Box::new((
                self.inner.clone(),
                GuiTask::SaveFileDialog {
                    title: title.to_string(),
                    default_name: default_name.map(|s| s.to_string()),
                    sender: tx,
                },
            ));
            unsafe {
                g_idle_add(idle_task_cb, Box::into_raw(task) as *mut c_void);
            }
            rx.recv()?
        }
    }
}

extern "C" fn destroy_window_cb(_widget: *mut c_void, _data: *mut c_void) {
    unsafe {
        gtk_main_quit();
    }
}

extern "C" fn on_context_menu_cb(
    _web_view: *mut c_void,
    _context_menu: *mut c_void,
    _event: *mut c_void,
    _hit_test_result: *mut c_void,
    _user_data: *mut c_void,
) -> c_int {
    1 // Return TRUE to suppress the default WebKit right-click context menu
}

extern "C" fn on_script_msg(_ucm: *mut c_void, js_result: *mut c_void, user_data: *mut c_void) {
    unsafe {
        if user_data.is_null() {
            return;
        }
        let data = user_data as *mut (WebViewHandle, MessageHandler);
        let (ref handle, ref handler) = *data;

        let val = webkit_javascript_result_get_js_value(js_result);
        let c_str = jsc_value_to_string(val);
        let msg = if !c_str.is_null() {
            let s = CStr::from_ptr(c_str).to_string_lossy().to_string();
            g_free(c_str as *mut _);
            s
        } else {
            String::new()
        };

        handler(handle.clone(), msg);
    }
}

extern "C" fn on_request_cb(request: *mut c_void, user_data: *mut c_void) {
    unsafe {
        if user_data.is_null() {
            return;
        }
        let data = user_data as *mut (WebViewHandle, RequestHandler);
        let (ref handle, ref handler) = *data;

        let uri_ptr = webkit_uri_scheme_request_get_uri(request);
        let uri_str = if !uri_ptr.is_null() {
            CStr::from_ptr(uri_ptr).to_string_lossy().to_string()
        } else {
            String::new()
        };

        let request_id = handle.proxy.inner.request_counter.fetch_add(1, Ordering::SeqCst).to_string();

        g_object_ref(request);
        let req_val = request as usize;
        {
            let mut requests = handle.proxy.inner.requests.lock().unwrap();
            requests.insert(request_id.clone(), req_val);
        }

        handler(handle.clone(), request_id, uri_str);
    }
}

pub fn run(mut builder: WebViewBuilder) -> Result<()> {
    // Dynamically load WebKit2GTK runtime libraries and symbols before making webview
    let _vt = vtable()?;

    let title_cstr = CString::new(builder.title)
        .map_err(|e| format_err!("Invalid window title string: {}", e))?;
    let is_devtools = builder.devtools;
    let auto_open_devtools = builder.devtools_auto_open;

    let inner = Arc::new(LinuxProxyInner {
        web_view: AtomicPtr::new(ptr::null_mut()),
        window: AtomicPtr::new(ptr::null_mut()),
        main_thread: std::thread::current().id(),
        requests: Default::default(),
        request_counter: std::sync::atomic::AtomicUsize::new(0),
    });

    let proxy = WebViewProxy { inner: inner.clone() };
    let handle = WebViewHandle { proxy };

    unsafe {
        gtk_init(ptr::null_mut(), ptr::null_mut());

        let window = gtk_window_new(0);
        if window.is_null() {
            return Err(err_msg("Failed to create GTK window."));
        }

        inner.window.store(window, Ordering::SeqCst);
        gtk_window_set_title(window, title_cstr.as_ptr());
        gtk_window_set_default_size(window, builder.width as c_int, builder.height as c_int);

        if builder.prefer_dark_theme {
            let gtk_settings = gtk_settings_get_default();
            if !gtk_settings.is_null() {
                let prop_name = CString::new("gtk-application-prefer-dark-theme").unwrap();
                (get_vtable().g_object_set)(gtk_settings, prop_name.as_ptr(), 1i32, ptr::null_mut::<c_void>());
            }
        }

        if builder.maximized {
            gtk_window_maximize(window);
        }

        let destroy_signal = CString::new("destroy").unwrap();
        g_signal_connect_data(
            window,
            destroy_signal.as_ptr(),
            destroy_window_cb as *mut c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );

        let web_view = webkit_web_view_new();
        if web_view.is_null() {
            return Err(err_msg("Failed to create WebKitWebView instance."));
        }

        let settings = webkit_web_view_get_settings(web_view);
        if !settings.is_null() {
            webkit_settings_set_enable_write_console_messages_to_stdout(settings, 1);
            webkit_settings_set_disable_web_security(settings, 1);
            webkit_settings_set_allow_file_access_from_file_urls(settings, 1);
            webkit_settings_set_allow_universal_access_from_file_urls(settings, 1);
            if is_devtools {
                webkit_settings_set_enable_developer_extras(settings, 1);
            }
        }

        inner.web_view.store(web_view, Ordering::SeqCst);

        if !builder.enable_context_menu && !is_devtools {
            let sig_name = CString::new("context-menu").unwrap();
            g_signal_connect_data(
                web_view,
                sig_name.as_ptr(),
                on_context_menu_cb as *mut c_void,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            );
        }

        // Register script message handler if provided
        if let Some(handler) = builder.on_message.take() {
            let ucm = webkit_web_view_get_user_content_manager(web_view);
            let ipc_name = CString::new("ipc").unwrap();
            webkit_user_content_manager_register_script_message_handler(ucm, ipc_name.as_ptr());

            let sig_name = CString::new("script-message-received::ipc").unwrap();
            let cb_data = Box::into_raw(Box::new((handle.clone(), handler))) as *mut c_void;
            g_signal_connect_data(
                ucm,
                sig_name.as_ptr(),
                on_script_msg as *mut c_void,
                cb_data,
                ptr::null_mut(),
                0,
            );
        }

        // Register webview URI scheme if provided
        if let Some(handler) = builder.on_request.take() {
            let scheme_cstr = CString::new(crate::CUSTOM_SCHEME).unwrap();
            let context = webkit_web_view_get_context(web_view);
            let sm = webkit_web_context_get_security_manager(context);
            if !sm.is_null() {
                webkit_security_manager_register_uri_scheme_as_secure(sm, scheme_cstr.as_ptr());
                webkit_security_manager_register_uri_scheme_as_cors_enabled(sm, scheme_cstr.as_ptr());
                webkit_security_manager_register_uri_scheme_as_local(sm, scheme_cstr.as_ptr());
            }
            let cb_data = Box::into_raw(Box::new((handle.clone(), handler))) as *mut c_void;
            webkit_web_context_register_uri_scheme(
                context,
                scheme_cstr.as_ptr(),
                on_request_cb,
                cb_data,
                ptr::null_mut(),
            );
        }

        let content_cstr = CString::new(builder.html)
            .map_err(|e| format_err!("Invalid HTML content string: {}", e))?;
        let base_uri_cstr = CString::new(crate::CUSTOM_SCHEME_URL).unwrap();

        webkit_web_view_load_html(web_view, content_cstr.as_ptr(), base_uri_cstr.as_ptr());

        gtk_container_add(window, web_view);
        gtk_widget_show_all(window);

        if is_devtools && auto_open_devtools {
            webkit_web_inspector_show(webkit_web_view_get_inspector(web_view));
        }

        if let Some(init_cb) = builder.on_init.take() {
            init_cb(handle.clone());
        }

        gtk_main();
    }

    Ok(())
}
