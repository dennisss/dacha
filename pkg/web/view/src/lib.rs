mod platform;

use std::sync::Arc;

use base_error::*;

pub type MessageHandler = Arc<dyn Fn(WebViewHandle, String) + Send + Sync + 'static>;
pub type RequestHandler = Arc<dyn Fn(WebViewHandle, String, String) + Send + Sync + 'static>;
pub type InitHandler = Box<dyn FnOnce(WebViewHandle) + Send + 'static>;

// TODO: Instead we should make these private but strip the prefix from all received requets.
pub const CUSTOM_SCHEME: &str = "webview";
pub const CUSTOM_SCHEME_URL: &str = "webview://localhost/";
pub const CUSTOM_SCHEME_INDEX_URL: &str = "webview://localhost/index.html";
pub const CUSTOM_SCHEME_PREFIX: &str = "webview://";

#[derive(Clone)]
pub struct WebViewHandle {
    pub(crate) proxy: platform::WebViewProxy,
}

impl WebViewHandle {
    /// Send a text or JSON message to the JavaScript renderer via native engine IPC.
    /// In JS, receive messages by listening to window.__on_message(msg).
    pub fn post_message(&self, message: &str) -> Result<()> {
        self.proxy.post_message(message)
    }

    /// Execute arbitrary JavaScript inside the Webview asynchronously.
    pub fn eval_js(&self, script: &str) -> Result<()> {
        self.proxy.eval_js(script)
    }

    /// Push binary streaming data to an active shared memory buffer (Windows zero-copy only).
    pub fn send_binary(&self, stream_id: &str, data: &[u8]) -> Result<()> {
        self.proxy.send_binary(stream_id, data)
    }

    /// Check if the native engine supports zero-copy shared buffers for binary data.
    pub fn supports_shared_buffer(&self) -> bool {
        cfg!(target_os = "windows")
    }

    /// Fulfill an asynchronous `webview://` request intercepted by `on_request`.
    pub fn send_response(&self, request_id: &str, status_code: u16, mime_type: &str, body: &[u8]) -> Result<()> {
        self.proxy.send_response(request_id, status_code, mime_type, body)
    }

    /// Open a native system file selection modal dialog to pick an existing file.
    /// Returns `Ok(Some(path))` if a file was selected, or `Ok(None)` if cancelled by the user.
    pub fn open_file_dialog(&self, title: &str) -> Result<Option<String>> {
        self.proxy.open_file_dialog(title)
    }

    /// Open a native system save modal dialog to pick a destination file path.
    /// Returns `Ok(Some(path))` if a location was selected, or `Ok(None)` if cancelled by the user.
    pub fn save_file_dialog(&self, title: &str, default_name: Option<&str>) -> Result<Option<String>> {
        self.proxy.save_file_dialog(title, default_name)
    }
}

#[derive(Clone)]
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub http_only: bool,
    pub secure: bool,
}

pub struct WebViewBuilder {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub html: Option<String>,
    pub url: Option<String>,
    pub on_message: Option<MessageHandler>,
    pub on_request: Option<RequestHandler>,
    pub(crate) on_init: Option<InitHandler>,
    pub enable_context_menu: bool,
    pub devtools: bool,
    pub devtools_auto_open: bool,
    pub prefer_dark_theme: bool,
    pub maximized: bool,
    pub user_data_dir: Option<String>,
    pub icon: Option<Icon>,
    pub cookies: Vec<Cookie>,
}

impl WebViewBuilder {
    /// Create a new WebViewBuilder with specified window dimensions and title.
    /// By default, default browser context menus (right-click) are disabled.
    pub fn new(title: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_string(),
            width,
            height,
            html: None,
            url: None,
            on_message: None,
            on_request: None,
            on_init: None,
            enable_context_menu: false,
            devtools: false,
            devtools_auto_open: false,
            prefer_dark_theme: false,
            maximized: false,
            user_data_dir: None,
            icon: None,
            cookies: Vec::new(),
        }
    }

    /// Set the content of the WebView to load an HTML string directly.
    pub fn load_html(mut self, html: &str) -> Self {
        self.html = Some(html.to_string());
        self
    }

    /// Set the URL for the WebView to navigate to.
    /// If `load_html` is also used, this sets the base URL for the HTML content.
    /// If only `load_url` is used, the WebView will navigate to this URL natively.
    pub fn load_url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    /// Register a callback to handle messages sent from JavaScript via native engine IPC.
    /// In JS: Call window.webkit.messageHandlers.ipc.postMessage(msg) or window.chrome.webview.postMessage(msg).
    pub fn on_message<F>(mut self, handler: F) -> Self
    where
        F: Fn(WebViewHandle, String) + Send + Sync + 'static,
    {
        self.on_message = Some(Arc::new(handler));
        self
    }

    /// Register a callback to intercept asynchronous `webview://` fetches and navigation requests.
    /// The handler receives `(WebViewHandle, request_id, path)`. 
    /// You must call `handle.send_response(request_id, ...)` later.
    pub fn on_request<F>(mut self, handler: F) -> Self
    where
        F: Fn(WebViewHandle, String, String) + Send + Sync + 'static,
    {
        self.on_request = Some(Arc::new(handler));
        self
    }

    /// Register an initialization callback that receives a thread-safe WebViewHandle immediately
    /// upon window creation, allowing background async runtimes to retain a proxy to the window.
    pub fn on_init<F>(mut self, handler: F) -> Self
    where
        F: FnOnce(WebViewHandle) + Send + 'static,
    {
        self.on_init = Some(Box::new(handler));
        self
    }

    /// Enable or disable the default browser right-click context menu (default is disabled).
    pub fn with_enable_context_menu(mut self, enable: bool) -> Self {
        self.enable_context_menu = enable;
        self
    }

    /// Enables or disables the developer tools inspector window natively
    pub fn with_devtools(mut self, enabled: bool) -> Self {
        self.devtools = enabled;
        self
    }

    /// Automatically opens the devtools window when the webview is created
    pub fn with_devtools_auto_open(mut self, auto_open: bool) -> Self {
        self.devtools_auto_open = auto_open;
        self
    }

    /// Sets the native window title bar to prefer dark theme (where supported)
    pub fn with_prefer_dark_theme(mut self, prefer: bool) -> Self {
        self.prefer_dark_theme = prefer;
        self
    }

    /// Set the window icon (shows in the taskbar/dock and titlebar).
    /// The `rgba` vector should contain raw RGBA pixel data (4 bytes per pixel).
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Add a cookie to be injected into the webview before page load.
    pub fn with_cookie(mut self, name: &str, value: &str, domain: &str, path: &str, http_only: bool, secure: bool) -> Self {
        self.cookies.push(Cookie {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.to_string(),
            path: path.to_string(),
            http_only,
            secure,
        });
        self
    }

    /// Defaults the window to launch in full screen (maximized)
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Sets the directory where the webview will store its user data (e.g. cookies, cache).
    /// On Windows, this overrides the default WebView2 temporary directory.
    pub fn with_user_data_dir(mut self, path: &str) -> Self {
        self.user_data_dir = Some(path.to_string());
        self
    }

    /// Build and launch the native WebView window, blocking on the GUI event loop.
    pub fn run(self) -> Result<()> {
        if self.user_data_dir.is_none() {
            return Err(err_msg("user_data_dir must be explicitly provided (e.g. via .with_user_data_dir())."));
        }
        platform::run(self)
    }
}

/// Show a native system error dialog. This is safe to call before the webview is initialized.
pub fn show_error_dialog(title: &str, message: &str) {
    platform::show_error_dialog(title, message);
}
