use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::mem;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ptr;

use base_error::*;
use common::hash::FastHasherBuilder;
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH, SHCreateMemStream
};
use windows::Win32::System::WinRT::EventRegistrationToken;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler,
    WebMessageReceivedEventHandler,
    WebResourceRequestedEventHandler,
    take_pwstr,
    CoreWebView2EnvironmentOptions,
    CoreWebView2CustomSchemeRegistration,
};

use crate::{WebViewBuilder, WebViewHandle};


const WM_WEBVIEW_TASK: u32 = WM_APP + 1;

fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub(crate) struct ComPtr<T>(pub(crate) Option<T>);
unsafe impl<T> Send for ComPtr<T> {}
unsafe impl<T> Sync for ComPtr<T> {}

impl<T> Default for ComPtr<T> {
    fn default() -> Self {
        Self(None)
    }
}

pub(crate) struct WindowsProxyInner {
    pub(crate) hwnd: AtomicPtr<core::ffi::c_void>,
    pub(crate) main_thread: std::thread::ThreadId,
    pub(crate) web_view: Mutex<ComPtr<ICoreWebView2>>,
    pub(crate) env12: Mutex<ComPtr<ICoreWebView2Environment12>>,
    pub(crate) shared_buffer: Mutex<ComPtr<ICoreWebView2SharedBuffer>>,
    pub(crate) requests: Mutex<HashMap<
        String,
        (ComPtr<ICoreWebView2WebResourceRequestedEventArgs>, ComPtr<ICoreWebView2Deferral>),
        FastHasherBuilder    
    >>,
    pub(crate) request_counter: std::sync::atomic::AtomicUsize,
}

#[derive(Clone)]
pub struct WebViewProxy {
    pub(crate) inner: Arc<WindowsProxyInner>,
}

enum GuiTask {
    EvalJs(String),
    SendBinary { stream_id: String, data: Vec<u8> },
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

impl WindowsProxyInner {
    fn handle_eval_js(&self, script: &str) {
        let wv_lock = self.web_view.lock().unwrap();
        if let Some(wv) = &wv_lock.0 {
            let _ = unsafe { wv.ExecuteScript(&HSTRING::from(script), None) };
        }
    }

    fn handle_send_binary(&self, stream_id: &str, data: &[u8]) {
        let wv_lock = self.web_view.lock().unwrap();
        let env_lock = self.env12.lock().unwrap();
        let mut buf_lock = self.shared_buffer.lock().unwrap();

        if let (Some(wv), Some(env12)) = (&wv_lock.0, &env_lock.0) {
            if let Ok(wv17) = wv.cast::<ICoreWebView2_17>() {
                let required_size = std::cmp::max(data.len() as u64, 1024 * 1024);
                let mut needs_new = true;
                if let Some(buf) = &buf_lock.0 {
                    let mut sz: u64 = 0;
                    if unsafe { buf.Size(&mut sz) }.is_ok() && sz >= data.len() as u64 {
                        needs_new = false;
                    }
                }
                if needs_new {
                    if let Ok(new_buf) = unsafe { env12.CreateSharedBuffer(required_size) } {
                        buf_lock.0 = Some(new_buf);
                    }
                }
                if let Some(buf) = &buf_lock.0 {
                    let mut raw_ptr: *mut u8 = ptr::null_mut();
                    if unsafe { buf.Buffer(&mut raw_ptr) }.is_ok() && !raw_ptr.is_null() && !data.is_empty() {
                        unsafe {
                            ptr::copy_nonoverlapping(data.as_ptr(), raw_ptr, data.len());
                        }
                    }
                    let json = format!("{{\"stream_id\":\"{}\",\"len\":{}}}", stream_id, data.len());
                    let _ = unsafe {
                        wv17.PostSharedBufferToScript(
                            buf,
                            COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY,
                            &HSTRING::from(json.as_str()),
                        )
                    };
                    return;
                }
            }
        }

        eprintln!("Warning: send_binary called for stream '{}', but PostSharedBufferToScript / shared buffer COM interface was unavailable. Dropping binary payload to prevent silent performance degradation.", stream_id);
    }

    fn run_open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        unsafe {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create FileOpenDialog: {}", e))?;
            let title_h = HSTRING::from(title);
            let _ = dialog.SetTitle(&title_h);
            let hwnd_ptr = self.hwnd.load(Ordering::SeqCst);
            if dialog.Show(HWND(hwnd_ptr as *mut _)).is_ok() {
                if let Ok(item) = dialog.GetResult() {
                    if let Ok(pwstr) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                        let path_str = pwstr.to_string().map_err(|e| e.to_string())?;
                        return Ok(Some(path_str));
                    }
                }
            }
            Ok(None)
        }
    }

    fn run_save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        unsafe {
            let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create FileSaveDialog: {}", e))?;
            let title_h = HSTRING::from(title);
            let _ = dialog.SetTitle(&title_h);
            if let Some(name) = default_name {
                let name_h = HSTRING::from(name);
                let _ = dialog.SetFileName(&name_h);
            }
            let hwnd_ptr = self.hwnd.load(Ordering::SeqCst);
            if dialog.Show(HWND(hwnd_ptr as *mut _)).is_ok() {
                if let Ok(item) = dialog.GetResult() {
                    if let Ok(pwstr) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                        let path_str = pwstr.to_string().map_err(|e| e.to_string())?;
                        return Ok(Some(path_str));
                    }
                }
            }
            Ok(None)
        }
    }

    fn handle_send_response(&self, request_id: &str, status_code: u16, mime_type: &str, body: &[u8]) {
        let req_opt = {
            let mut reqs = self.requests.lock().unwrap();
            reqs.remove(request_id)
        };
        if let Some((args_ptr, deferral_ptr)) = req_opt {
            if let (Some(args), Some(deferral)) = (args_ptr.0, deferral_ptr.0) {
                let env_lock = self.env12.lock().unwrap();
                if let Some(env12) = &env_lock.0 {
                    unsafe {
                        if let Some(stream) = SHCreateMemStream(Some(body)) {
                            let header_str = format!("Content-Type: {}", mime_type);
                            let reason = match status_code {
                                200 => "OK",
                                404 => "Not Found",
                                400 => "Bad Request",
                                500 => "Internal Server Error",
                                _ => "Unknown",
                            };
                            if let Ok(response) = env12.CreateWebResourceResponse(
                                &stream,
                                status_code as i32,
                                &HSTRING::from(reason),
                                &HSTRING::from(header_str.as_str())
                            ) {
                                let _ = args.SetResponse(&response);
                            }
                        }
                    }
                }
                unsafe {
                    let _ = deferral.Complete();
                }
            }
        }
    }

    fn handle_abort_request(&self, request_id: &str) {
        let req_opt = {
            let mut reqs = self.requests.lock().unwrap();
            reqs.remove(request_id)
        };
        if let Some((args_ptr, deferral_ptr)) = req_opt {
            if let (Some(args), Some(deferral)) = (args_ptr.0, deferral_ptr.0) {
                let env_lock = self.env12.lock().unwrap();
                if let Some(env12) = &env_lock.0 {
                    unsafe {
                        if let Some(stream) = SHCreateMemStream(Some(&[])) {
                            if let Ok(response) = env12.CreateWebResourceResponse(
                                &stream,
                                500,
                                &HSTRING::from("Aborted"),
                                &HSTRING::from("")
                            ) {
                                let _ = args.SetResponse(&response);
                            }
                        }
                    }
                }
                unsafe {
                    let _ = deferral.Complete();
                }
            }
        }
    }
}

impl WebViewProxy {
    pub fn post_message(&self, message: &str) -> Result<()> {
        self.eval_js(&super::create_post_message_script(message))
    }

    pub fn eval_js(&self, script: &str) -> Result<()> {
        self.post_gui_task(GuiTask::EvalJs(script.to_string()))
    }

    pub fn send_binary(&self, stream_id: &str, data: &[u8]) -> Result<()> {
        self.post_gui_task(GuiTask::SendBinary {
            stream_id: stream_id.to_string(),
            data: data.to_vec(),
        })
    }

    pub fn send_response(&self, request_id: &str, status_code: u16, mime_type: &str, body: &[u8]) -> Result<()> {
        self.post_gui_task(GuiTask::SendResponse {
            request_id: request_id.to_string(),
            status_code,
            mime_type: mime_type.to_string(),
            body: body.to_vec(),
        })
    }

    pub fn abort_request(&self, request_id: &str) -> Result<()> {
        self.post_gui_task(GuiTask::AbortRequest {
            request_id: request_id.to_string(),
        })
    }

    fn post_gui_task(&self, task: GuiTask) -> Result<()> {
        let hwnd = self.inner.hwnd.load(Ordering::SeqCst);
        if !hwnd.is_null() {
            let task = Box::new((self.inner.clone(), task));
            unsafe {
                let _ = PostMessageW(HWND(hwnd as *mut _), WM_WEBVIEW_TASK, WPARAM(Box::into_raw(task) as _), LPARAM(0));
            }
        }
        Ok(())
    }

    pub fn open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        if std::thread::current().id() == self.inner.main_thread {
            self.inner.run_open_file_dialog(title)
        } else {
            let hwnd = self.inner.hwnd.load(Ordering::SeqCst);
            if hwnd.is_null() {
                return Err(err_msg("Window handle is null; cannot open file dialog"));
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let task = Box::new((
                self.inner.clone(),
                GuiTask::OpenFileDialog {
                    title: title.to_string(),
                    sender: tx,
                },
            ));
            unsafe {
                let _ = PostMessageW(HWND(hwnd as *mut _), WM_WEBVIEW_TASK, WPARAM(Box::into_raw(task) as _), LPARAM(0));
            }
            rx.recv()?
        }
    }

    pub fn save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        if std::thread::current().id() == self.inner.main_thread {
            self.inner.run_save_file_dialog(title, default_name)
        } else {
            let hwnd = self.inner.hwnd.load(Ordering::SeqCst);
            if hwnd.is_null() {
                return Err(err_msg("Window handle is null; cannot save file dialog"));
            }
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
                let _ = PostMessageW(HWND(hwnd as *mut _), WM_WEBVIEW_TASK, WPARAM(Box::into_raw(task) as _), LPARAM(0));
            }
            rx.recv()?
        }
    }
}

extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_SIZE => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ICoreWebView2Controller;
                if !ptr.is_null() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let _ = (*ptr).SetBounds(rect);
                }
                LRESULT(0)
            }
            WM_WEBVIEW_TASK => {
                if wparam.0 != 0 {
                    let box_data = Box::from_raw(wparam.0 as *mut (Arc<WindowsProxyInner>, GuiTask));
                    let (inner, task) = *box_data;
                    match task {
                        GuiTask::EvalJs(script) => {
                            inner.handle_eval_js(&script);
                        }
                        GuiTask::SendBinary { stream_id, data } => {
                            inner.handle_send_binary(&stream_id, &data);
                        }
                        GuiTask::SendResponse { request_id, status_code, mime_type, body } => {
                            inner.handle_send_response(&request_id, status_code, &mime_type, &body);
                        }
                        GuiTask::AbortRequest { request_id } => {
                            inner.handle_abort_request(&request_id);
                        }
                        GuiTask::OpenFileDialog { title, sender } => {
                            let _ = sender.send(inner.run_open_file_dialog(&title));
                        }
                        GuiTask::SaveFileDialog { title, default_name, sender } => {
                            let _ = sender.send(inner.run_save_file_dialog(&title, default_name.as_deref()));
                        }
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[derive(Clone)]
struct HandlerData {
    hwnd: HWND,
    inner: Arc<WindowsProxyInner>,
    handle: WebViewHandle,
    on_msg: Option<Arc<dyn Fn(WebViewHandle, String) + Send + Sync>>,
    on_request: Option<Arc<dyn Fn(WebViewHandle, String, String) + Send + Sync>>,
    enable_context_menu: bool,
    devtools: bool,
    devtools_auto_open: bool,
    html: String,
}

fn create_env_handler(
    data: HandlerData
) -> ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler {
    CreateCoreWebView2EnvironmentCompletedHandler::create(
        Box::new(move |result, env| {
            if result.is_err() || env.is_none() {
                return Ok(());
            }
            let env = env.as_ref().unwrap();
            if let Ok(e12) = env.cast::<ICoreWebView2Environment12>() {
                data.inner.env12.lock().unwrap().0 = Some(e12);
            }

            let env_clone = env.clone();
            let data_clone = data.clone();
            let ctrl_handler = create_ctrl_handler(data_clone, env_clone);
            
            unsafe {
                let _ = env.CreateCoreWebView2Controller(data.hwnd, &ctrl_handler);
            }
            Ok(())
        })
    )
}

fn create_ctrl_handler(
    data: HandlerData, env: ICoreWebView2Environment
) -> ICoreWebView2CreateCoreWebView2ControllerCompletedHandler {
    CreateCoreWebView2ControllerCompletedHandler::create(
        Box::new(move |result, ctrl| {
            if result.is_err() || ctrl.is_none() {
                return Ok(());
            }
            let ctrl = ctrl.as_ref().unwrap().clone();
            
            unsafe {
                let mut rect = RECT::default();
                let _ = GetClientRect(data.hwnd, &mut rect);
                let _ = ctrl.SetBounds(rect);
                let _ = ctrl.SetIsVisible(true);

                let ctrl_ptr = Box::into_raw(Box::new(ctrl.clone()));
                SetWindowLongPtrW(data.hwnd, GWLP_USERDATA, ctrl_ptr as isize);
            }

            let web_view: Option<ICoreWebView2> = unsafe { ctrl.CoreWebView2().ok() };
            if let Some(webview) = web_view {
                data.inner.web_view.lock().unwrap().0 = Some(webview.clone());

                unsafe {
                    if let Ok(settings) = webview.Settings() {
                        let _ = settings.SetAreDefaultContextMenusEnabled(data.enable_context_menu);
                        let _ = settings.SetAreDevToolsEnabled(data.devtools);
                    }
                    
                    if data.devtools_auto_open {
                        let _ = webview.OpenDevToolsWindow();
                    }

                    if let Some(handler) = &data.on_msg {
                        let msg_handler = create_msg_handler(data.handle.clone(), handler.clone());
                        let mut token = EventRegistrationToken::default();
                        let _ = webview.add_WebMessageReceived(&msg_handler, &mut token);
                    }

                    if let Some(handler) = &data.on_request {
                        let filter_str = format!("{}*", crate::CUSTOM_SCHEME_PREFIX);
                        let filter = HSTRING::from(filter_str);
                        let _ = webview.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
                        
                        let res_handler = create_req_handler(
                            data.handle.clone(),
                            data.inner.clone(),
                            env.clone(),
                            data.html.clone(),
                            handler.clone()
                        );
                        let mut token = EventRegistrationToken::default();
                        let _ = webview.add_WebResourceRequested(&res_handler, &mut token);
                    }

                    let _ = webview.Navigate(&HSTRING::from(crate::CUSTOM_SCHEME_URL));
                }
            }
            Ok(())
        })
    )
}

fn create_msg_handler(
    handle: WebViewHandle,
    handler_cb: Arc<dyn Fn(WebViewHandle, String) + Send + Sync>,
) -> ICoreWebView2WebMessageReceivedEventHandler {
    WebMessageReceivedEventHandler::create(
        Box::new(move |_wv, args| {
            if let Some(args) = args {
                unsafe {
                    let mut pwstr = PWSTR::null();
                    let _ = args.TryGetWebMessageAsString(&mut pwstr);
                    handler_cb(handle.clone(), take_pwstr(pwstr));
                }
            }
            Ok(())
        })
    )
}

fn create_req_handler(
    handle: WebViewHandle,
    inner: Arc<WindowsProxyInner>,
    env: ICoreWebView2Environment,
    html_content: String,
    handler_cb: Arc<dyn Fn(WebViewHandle, String, String) + Send + Sync>,
) -> ICoreWebView2WebResourceRequestedEventHandler {
    WebResourceRequestedEventHandler::create(
        Box::new(move |_wv, args| {
            if let Some(args) = args {
                unsafe {
                    if let Ok(req) = args.Request() {
                        let mut pwstr = PWSTR::null();
                        let _ = req.Uri(&mut pwstr);
                        let uri = take_pwstr(pwstr);
                        
                        if uri == crate::CUSTOM_SCHEME_URL || uri == crate::CUSTOM_SCHEME_INDEX_URL {
                            if let Some(stream) = SHCreateMemStream(Some(html_content.as_bytes())) {
                                if let Ok(response) = env.CreateWebResourceResponse(
                                    &stream,
                                    200,
                                    &HSTRING::from("OK"),
                                    &HSTRING::from("Content-Type: text/html\nAccess-Control-Allow-Origin: *")
                                ) {
                                    let _ = args.SetResponse(&response);
                                }
                            }
                        } else if uri.starts_with(crate::CUSTOM_SCHEME_PREFIX) {
                            let request_id = inner.request_counter.fetch_add(1, Ordering::SeqCst).to_string();
                            if let Ok(deferral) = args.GetDeferral() {
                                let mut reqs = inner.requests.lock().unwrap();
                                reqs.insert(request_id.clone(), (ComPtr(Some(args.clone())), ComPtr(Some(deferral))));
                            }
                            handler_cb(handle.clone(), request_id, uri);
                        }
                    }
                }
            }
            Ok(())
        })
    )
}

pub fn run(mut builder: WebViewBuilder) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let instance = GetModuleHandleW(None)?;;
        let class_name = to_wstring("MinimalWebViewClass");
        let title_w = to_wstring(&builder.title);

        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance.into(),
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
            hIconSm: HICON::default(),
        };

        if RegisterClassExW(&wc) == 0 {
            return Err(err_msg("Failed to register Win32 window class."));
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::from_raw(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            builder.width as i32,
            builder.height as i32,
            None,
            None,
            instance,
            None,
        ).map_err(|e| format_err!("Failed to create Win32 window: {}", e))?;

        let inner = Arc::new(WindowsProxyInner {
            hwnd: AtomicPtr::new(hwnd.0 as *mut _),
            main_thread: std::thread::current().id(),
            web_view: Mutex::new(ComPtr::default()),
            env12: Mutex::new(ComPtr::default()),
            shared_buffer: Mutex::new(ComPtr::default()),
            requests: Mutex::new(HashMap::new()),
            request_counter: std::sync::atomic::AtomicUsize::new(0),
        });
        let proxy = WebViewProxy { inner: inner.clone() };
        let handle = WebViewHandle { proxy };

        let mut show_mode = SW_SHOW;
        if builder.maximized {
            show_mode = SW_SHOWMAXIMIZED;
        }

        if builder.prefer_dark_theme {
            let mut value: i32 = 1;
            let res = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &mut value as *mut _ as *const core::ffi::c_void,
                std::mem::size_of_val(&value) as u32
            );
            if res.is_err() {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(19),
                    &mut value as *mut _ as *const core::ffi::c_void,
                    std::mem::size_of_val(&value) as u32
                );
            }
        }

        let _ = ShowWindow(hwnd, show_mode);
        let _ = UpdateWindow(hwnd);

        let hwnd_copy = hwnd;
        
        let handler_data = HandlerData {
            hwnd: hwnd_copy,
            inner: inner.clone(),
            handle: handle.clone(),
            on_msg: builder.on_message.take(),
            on_request: builder.on_request.take(),
            enable_context_menu: builder.enable_context_menu,
            devtools: builder.devtools,
            devtools_auto_open: builder.devtools_auto_open,
            html: builder.html,
        };

        let env_handler = create_env_handler(handler_data);


        let temp_dir_str = builder.user_data_dir.unwrap();
        let temp_dir_w = to_wstring(&temp_dir_str);
        
        let options = CoreWebView2EnvironmentOptions::default();
        let scheme = CoreWebView2CustomSchemeRegistration::new(crate::CUSTOM_SCHEME.to_string());
        scheme.set_treat_as_secure(true);
        scheme.set_has_authority_component(true);
        options.set_scheme_registrations(vec![Some(scheme.into())]);
        
        let options_com: ICoreWebView2EnvironmentOptions = options.into();

        let _ = CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            PCWSTR::from_raw(temp_dir_w.as_ptr()),
            &options_com,
            &env_handler,
        );

        if let Some(init_cb) = builder.on_init.take() {
            init_cb(handle);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}
