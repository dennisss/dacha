use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::ffi::c_void;
use std::collections::HashMap;

use base_error::*;
use common::hash::FastHasherBuilder;

use objc2::{ClassType, msg_send_id, msg_send, define_class, DefinedClass, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::{ProtocolObject, NSObject, NSObjectProtocol};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow,
    NSWindowStyleMask, NSOpenPanel, NSSavePanel,
    NSAppearance, NSAppearanceNameDarkAqua, NSAppearanceCustomization,
    NSAlert, NSAlertStyle, NSImage, NSBitmapImageRep
};
use objc2_foundation::{
    NSPoint, NSRect, NSSize, NSString, NSURL, MainThreadMarker,
    NSData, NSHTTPURLResponse, NSNumber, NSUUID
};
use objc2_web_kit::{
    WKWebView, WKWebViewConfiguration, WKURLSchemeHandler, WKURLSchemeTask,
    WKScriptMessageHandler, WKUserContentController, WKScriptMessage,
    WKUserScript, WKUserScriptInjectionTime, WKWebsiteDataStore
};

use crate::{WebViewBuilder, WebViewHandle, RequestHandler};

// Wrapper to make Retained WKURLSchemeTask Send and Sync
pub(crate) struct SendTask(pub Retained<ProtocolObject<dyn WKURLSchemeTask>>);
unsafe impl Send for SendTask {}
unsafe impl Sync for SendTask {}

pub(crate) struct MacosProxyInner {
    pub(crate) web_view: AtomicPtr<c_void>,
    pub(crate) requests: Mutex<HashMap<String, SendTask, FastHasherBuilder>>,
    pub(crate) request_counter: AtomicUsize,
    pub(crate) handler: Mutex<Option<RequestHandler>>,
    pub(crate) on_msg: Mutex<Option<Arc<dyn Fn(WebViewHandle, String) + Send + Sync>>>,
    pub(crate) proxy_clone: Mutex<Option<WebViewProxy>>,
}

#[derive(Clone)]
pub struct WebViewProxy {
    pub(crate) inner: Arc<MacosProxyInner>,
}

impl WebViewProxy {
    pub fn post_message(&self, message: &str) -> Result<()> {
        self.eval_js(&super::create_post_message_script(message))
    }

    pub fn eval_js(&self, script: &str) -> Result<()> {
        let ptr = self.inner.web_view.load(Ordering::SeqCst);
        if !ptr.is_null() {
            unsafe {
                let web_view = &*(ptr as *const WKWebView);
                let script_ns = NSString::from_str(script);
                web_view.evaluateJavaScript_completionHandler(&script_ns, None);
            }
        }
        Ok(())
    }

    pub fn send_binary(&self, _stream_id: &str, _data: &[u8]) -> Result<()> {
        Err(err_msg("Binary streams are not supported on macOS"))
    }

    pub fn send_response(&self, request_id: &str, status_code: u16, _mime_type: &str, body: &[u8]) -> Result<()> {
        let task = {
            let mut map = self.inner.requests.lock().unwrap();
            map.remove(request_id)
        };
        if let Some(task) = task {
            unsafe {
                let req = task.0.request();
                if let Some(url) = req.URL() {
                    let response_alloc: objc2::rc::Allocated<NSHTTPURLResponse> = msg_send_id![NSHTTPURLResponse::class(), alloc];
                    let response = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
                        response_alloc,
                        &url,
                        status_code as isize,
                        None,
                        None
                    );
                    if let Some(response) = response {
                        task.0.didReceiveResponse(&response);
                        let data = NSData::with_bytes(body);
                        task.0.didReceiveData(&data);
                        task.0.didFinish();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        unsafe {
            let mtm = MainThreadMarker::new_unchecked();
            let panel = NSOpenPanel::openPanel(mtm);
            let title_ns = NSString::from_str(title);
            panel.setTitle(Some(&title_ns));
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(false);
            if panel.runModal() == 1 { // NSModalResponseOK is 1
                if let Some(url) = panel.URL() {
                    if let Some(path) = url.path() {
                        return Ok(Some(path.to_string()));
                    }
                }
            }
            Ok(None)
        }
    }

    pub fn save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        unsafe {
            let mtm = MainThreadMarker::new_unchecked();
            let panel = NSSavePanel::savePanel(mtm);
            let title_ns = NSString::from_str(title);
            panel.setTitle(Some(&title_ns));
            if let Some(name) = default_name {
                let name_ns = NSString::from_str(name);
                panel.setNameFieldStringValue(&name_ns);
            }
            if panel.runModal() == 1 { // NSModalResponseOK is 1
                if let Some(url) = panel.URL() {
                    if let Some(path) = url.path() {
                        return Ok(Some(path.to_string()));
                    }
                }
            }
            Ok(None)
        }
    }
}

pub struct CustomSchemeHandlerIvars {
    pub inner: Arc<MacosProxyInner>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "CustomSchemeHandler"]
    #[thread_kind = MainThreadOnly]
    #[ivars = CustomSchemeHandlerIvars]
    pub struct CustomSchemeHandler;

    unsafe impl NSObjectProtocol for CustomSchemeHandler {}

    unsafe impl WKURLSchemeHandler for CustomSchemeHandler {
        #[unsafe(method(webView:startURLSchemeTask:))]
        unsafe fn webView_startURLSchemeTask(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let inner = &self.ivars().inner;
            let req_id = inner.request_counter.fetch_add(1, Ordering::SeqCst).to_string();
            
            let url = task.request().URL().unwrap().absoluteString().unwrap().to_string();
            
            let retained_task = objc2::rc::Retained::retain(task as *const _ as *mut _).unwrap();
            inner.requests.lock().unwrap().insert(req_id.clone(), SendTask(retained_task));
            
            let handler = inner.handler.lock().unwrap().clone();
            let proxy_clone = inner.proxy_clone.lock().unwrap().clone();
            
            if let Some(handler) = handler {
                if let Some(proxy) = proxy_clone {
                    handler(WebViewHandle { proxy }, req_id, url);
                }
            }
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        unsafe fn webView_stopURLSchemeTask(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let inner = &self.ivars().inner;
            let mut map = inner.requests.lock().unwrap();
            let mut found = None;
            for (id, t) in map.iter() {
                if std::ptr::eq(objc2::rc::Retained::as_ptr(&t.0) as *const c_void, task as *const ProtocolObject<dyn WKURLSchemeTask> as *const c_void) {
                    found = Some(id.clone());
                    break;
                }
            }
            if let Some(id) = found {
                map.remove(&id);
            }
        }
    }
);

impl CustomSchemeHandler {
    pub fn new(mtm: MainThreadMarker, inner: Arc<MacosProxyInner>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(CustomSchemeHandlerIvars { inner });
        unsafe { msg_send_id![super(this), init] }
    }
}

pub struct ScriptMessageHandlerIvars {
    pub inner: Arc<MacosProxyInner>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ScriptMessageHandler"]
    #[thread_kind = MainThreadOnly]
    #[ivars = ScriptMessageHandlerIvars]
    pub struct ScriptMessageHandler;

    unsafe impl NSObjectProtocol for ScriptMessageHandler {}

    unsafe impl WKScriptMessageHandler for ScriptMessageHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        unsafe fn userContentController_didReceiveScriptMessage(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            let body = message.body();
            let is_string: bool = objc2::msg_send![&body, isKindOfClass: NSString::class()];
            if is_string {
                let string_ptr = objc2::rc::Retained::as_ptr(&body) as *const NSString;
                let ns_string = &*string_ptr;
                let rust_string = ns_string.to_string();
                
                let inner = &self.ivars().inner;
                let handler = inner.on_msg.lock().unwrap().clone();
                let proxy_clone = inner.proxy_clone.lock().unwrap().clone();
                if let Some(handler) = handler {
                    if let Some(proxy) = proxy_clone {
                        handler(WebViewHandle { proxy }, rust_string);
                    }
                }
            }
        }
    }
);

impl ScriptMessageHandler {
    pub fn new(mtm: MainThreadMarker, inner: Arc<MacosProxyInner>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ScriptMessageHandlerIvars { inner });
        unsafe { msg_send_id![super(this), init] }
    }
}


pub fn run(mut builder: WebViewBuilder) -> Result<()> {
    let _on_msg = builder.on_message.take();
    let inner = Arc::new(MacosProxyInner {
        web_view: AtomicPtr::new(std::ptr::null_mut()),
        requests: Default::default(),
        request_counter: AtomicUsize::new(0),
        handler: Mutex::new(builder.on_request.take()),
        on_msg: Mutex::new(_on_msg),
        proxy_clone: Mutex::new(None),
    });
    let proxy = WebViewProxy { inner: inner.clone() };
    let handle = WebViewHandle { proxy: proxy.clone() };
    *inner.proxy_clone.lock().unwrap() = Some(proxy.clone());

    let on_init = builder.on_init.take();
    let html_content = builder.html.clone();
    let url_content = builder.url.clone();

    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(builder.width as f64, builder.height as f64),
        );
        
        if let Some(ref icon) = builder.icon {
            let color_space = NSString::from_str("NSDeviceRGBColorSpace");
            let rep_alloc: objc2::rc::Allocated<NSBitmapImageRep> = msg_send_id![NSBitmapImageRep::class(), alloc];
            let rep: Option<Retained<NSBitmapImageRep>> = unsafe {
                msg_send_id![
                    rep_alloc,
                    initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>(),
                    pixelsWide: icon.width as isize,
                    pixelsHigh: icon.height as isize,
                    bitsPerSample: 8 as isize,
                    samplesPerPixel: 4 as isize,
                    hasAlpha: true,
                    isPlanar: false,
                    colorSpaceName: &*color_space,
                    bitmapFormat: 0 as isize,
                    bytesPerRow: (icon.width * 4) as isize,
                    bitsPerPixel: 32 as isize,
                ]
            };
            if let Some(rep) = rep {
                unsafe {
                    let ptr: *mut u8 = msg_send![&rep, bitmapData];
                    if !ptr.is_null() {
                        std::ptr::copy_nonoverlapping(icon.rgba.as_ptr(), ptr, icon.rgba.len());
                    }
                }
                let image_alloc: objc2::rc::Allocated<NSImage> = msg_send_id![NSImage::class(), alloc];
                let size = NSSize::new(icon.width as f64, icon.height as f64);
                let image: Retained<NSImage> = unsafe { msg_send_id![image_alloc, initWithSize: size] };
                unsafe {
                    let _: () = msg_send![&image, addRepresentation: &*rep];
                }
                NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
            }
        }

        let style_mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::Miniaturizable;

        let window_alloc: objc2::rc::Allocated<NSWindow> = msg_send_id![NSWindow::class(), alloc];
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            window_alloc,
            frame,
            style_mask,
            NSBackingStoreType::Buffered,
            false,
        );

        let title_ns = NSString::from_str(&builder.title);
        window.setTitle(&title_ns);
        window.center();

        if builder.prefer_dark_theme {
            if let Some(appearance) = NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) {
                window.setAppearance(Some(&appearance));
            }
        }

        let _enable_context_menu = builder.enable_context_menu;
        let _devtools = builder.devtools;
        let config = WKWebViewConfiguration::new();
        
        if _devtools {
            let prefs = config.preferences();
            let key = NSString::from_str("developerExtrasEnabled");
            let val = NSNumber::numberWithBool(true);
            let _: () = msg_send![&prefs, setValue: &*val, forKey: &*key];
        }
        
        let scheme_handler = CustomSchemeHandler::new(mtm, inner.clone());
        let scheme_handler_proto: &ProtocolObject<dyn WKURLSchemeHandler> = ProtocolObject::from_ref(&*scheme_handler);
        let scheme_name = NSString::from_str(crate::CUSTOM_SCHEME);
        config.setURLSchemeHandler_forURLScheme(
            Some(scheme_handler_proto),
            &scheme_name,
        );
        
        let user_content_controller = config.userContentController();
        
        let msg_handler = ScriptMessageHandler::new(mtm, inner.clone());
        let msg_handler_proto: &ProtocolObject<dyn WKScriptMessageHandler> = ProtocolObject::from_ref(&*msg_handler);
        let name_ns = NSString::from_str("ipc");
        user_content_controller.addScriptMessageHandler_name(msg_handler_proto, &name_ns);
        

        if !_enable_context_menu && !_devtools {
            let ctx_script = NSString::from_str("window.addEventListener('contextmenu', (e) => { e.preventDefault(); });");
            let user_script_alloc: objc2::rc::Allocated<WKUserScript> = msg_send_id![WKUserScript::class(), alloc];
            let ctx_user_script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                user_script_alloc,
                &ctx_script,
                WKUserScriptInjectionTime(0),
                true
            );
            user_content_controller.addUserScript(&ctx_user_script);
        }

        let web_view_alloc: objc2::rc::Allocated<WKWebView> = msg_send_id![WKWebView::class(), alloc];
        let web_view = WKWebView::initWithFrame_configuration(
            web_view_alloc,
            frame,
            &config,
        );

        if _devtools {
            let sel = objc2::sel!(setInspectable:);
            let responds: bool = msg_send![&web_view, respondsToSelector: sel];
            if responds {
                let _: () = msg_send![&web_view, setInspectable: true];
            }
        }

        inner.web_view.store(objc2::rc::Retained::as_ptr(&web_view) as *mut c_void, Ordering::SeqCst);

        let website_data_store: Retained<objc2::runtime::AnyObject> = msg_send_id![&config, websiteDataStore];
        let cookie_store: Retained<objc2::runtime::AnyObject> = msg_send_id![&website_data_store, httpCookieStore];

        let load_content = {
            let web_view = web_view.clone();
            let html_content = html_content.clone();
            let url_content = url_content.clone();
            move || {
                unsafe {
                    if let Some(html) = &html_content {
                        let content_ns = NSString::from_str(&html);
                        let base_url = url_content.clone().unwrap_or_else(|| crate::CUSTOM_SCHEME_URL.to_string());
                        let base_url_ns = NSString::from_str(&base_url);
                        let ns_url = NSURL::URLWithString(&base_url_ns);
                        let _: () = msg_send![&web_view, loadHTMLString: &*content_ns, baseURL: ns_url.as_deref()];
                    } else if let Some(url) = &url_content {
                        let url_ns = NSString::from_str(&url);
                        let ns_url = NSURL::URLWithString(&url_ns).unwrap();
                        let request: objc2::rc::Retained<objc2::runtime::AnyObject> = msg_send_id![objc2::class!(NSURLRequest), requestWithURL: &*ns_url];
                        let _: () = msg_send![&web_view, loadRequest: &*request];
                    }
                }
            }
        };

        let cookies = builder.cookies.clone();
        if cookies.is_empty() {
            load_content();
        } else {
            let mut next_block = block2::RcBlock::new(load_content);
            fn create_ns_cookie(cookie: &crate::Cookie) -> Retained<objc2::runtime::AnyObject> {
                unsafe {
                    let props: Retained<objc2::runtime::AnyObject> = msg_send_id![objc2::class!(NSMutableDictionary), dictionary];
                    let _: () = msg_send![&props, setObject: &*NSString::from_str(&cookie.name), forKey: &*NSString::from_str("NSHTTPCookieName")];
                    let _: () = msg_send![&props, setObject: &*NSString::from_str(&cookie.value), forKey: &*NSString::from_str("NSHTTPCookieValue")];
                    let _: () = msg_send![&props, setObject: &*NSString::from_str(&cookie.domain), forKey: &*NSString::from_str("NSHTTPCookieDomain")];
                    let _: () = msg_send![&props, setObject: &*NSString::from_str(&cookie.path), forKey: &*NSString::from_str("NSHTTPCookiePath")];
                    if cookie.secure {
                        let true_num: Retained<objc2::runtime::AnyObject> = msg_send_id![objc2::class!(NSNumber), numberWithBool: true];
                        let _: () = msg_send![&props, setObject: &*true_num, forKey: &*NSString::from_str("NSHTTPCookieSecure")];
                    }
                    if cookie.http_only {
                        let true_num: Retained<objc2::runtime::AnyObject> = msg_send_id![objc2::class!(NSNumber), numberWithBool: true];
                        let _: () = msg_send![&props, setObject: &*true_num, forKey: &*NSString::from_str("HttpOnly")];
                    }
                    msg_send_id![objc2::class!(NSHTTPCookie), cookieWithProperties: &*props]
                }
            }

            let mut iter = cookies.into_iter().rev();
            let first_cookie = iter.next().unwrap();
            
            for cookie in iter {
                let current_next = next_block.clone();
                let cookie_store_clone = cookie_store.clone();
                
                next_block = block2::RcBlock::new(move || {
                    let ns_cookie = create_ns_cookie(&cookie);
                    unsafe {
                        let _: () = msg_send![&cookie_store_clone, setCookie: &*ns_cookie, completionHandler: &*current_next];
                    }
                });
            }
            
            let ns_cookie = create_ns_cookie(&first_cookie);
            unsafe {
                let _: () = msg_send![&cookie_store, setCookie: &*ns_cookie, completionHandler: &*next_block];
            }
        }

        window.setContentView(Some(&web_view));
        window.makeKeyAndOrderFront(None);

        if builder.maximized {
            window.zoom(None);
        }

        if let Some(init_cb) = on_init {
            init_cb(handle);
        }

        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        app.run();
    }

    Ok(())
}

pub fn show_error_dialog(title: &str, message: &str) {
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let _app: Retained<NSApplication> = NSApplication::sharedApplication(mtm);
        let alert_alloc: objc2::rc::Allocated<NSAlert> = msg_send_id![objc2::class!(NSAlert), alloc];
        let alert: Retained<NSAlert> = msg_send_id![alert_alloc, init];
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(message));
        alert.setAlertStyle(NSAlertStyle::Critical);
        alert.runModal();
    }
}
