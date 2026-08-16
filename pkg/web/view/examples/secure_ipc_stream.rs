#![windows_subsystem = "windows"]

use webview::WebViewBuilder;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

const INDEX_HTML: &str = r#"
<!DOCTYPE html>
<html>
<body>
    <h1>IPC and Streaming</h1>
    <button onclick="window.webkit?.messageHandlers?.ipc?.postMessage('ping') || window.chrome?.webview?.postMessage('ping')">Send Ping</button>
    <div id="log"></div>
    <script>
        window.__on_message = (msg) => {
            document.getElementById('log').innerHTML += `<p>Message: ${msg}</p>`;
        };
        
        async function startStream() {
            while (true) {
                try {
                    const response = await fetch('webview://localhost/stream');
                    if (response.ok) {
                        const buffer = await response.arrayBuffer();
                        document.getElementById('log').innerHTML += `<p>Stream chunk: ${buffer.byteLength} bytes</p>`;
                    }
                } catch (e) {
                    await new Promise(r => setTimeout(r, 1000));
                }
            }
        }
        setTimeout(startStream, 500);

        if (window.chrome && window.chrome.webview && window.chrome.webview.addEventListener) {
            window.chrome.webview.addEventListener('sharedbufferreceived', e => {
                const data = e.additionalData;
                document.getElementById('log').innerHTML += `<p>Shared buffer chunk: ${data.len} bytes</p>`;
            });
        }
    </script>
</body>
</html>
"#;

fn main() {
    let pending_req = Arc::new(Mutex::new(None::<String>));
    let pending_req_clone = pending_req.clone();

    WebViewBuilder::new("Secure IPC Stream Example", 800, 600)
        .with_user_data_dir(std::env::temp_dir().join("webview_example_data").to_str().unwrap())
        .load_html(INDEX_HTML)
        .on_request(move |handle, request_id, uri| {
            if uri.contains("stream") {
                *pending_req_clone.lock().unwrap() = Some(request_id);
            } else {
                let _ = handle.send_response(&request_id, 200, "text/plain", b"OK");
            }
        })
        .on_message(|handle, msg| {
            if msg == "ping" {
                let _ = handle.post_message("pong");
            }
        })
        .on_init(move |handle| {
            let handle_clone = handle.clone();
            let pending = pending_req.clone();
            thread::spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(1));
                    let chunk = vec![0u8; 1024];
                    if let Some(req_id) = pending.lock().unwrap().take() {
                        let _ = handle_clone.send_response(&req_id, 200, "application/octet-stream", &chunk);
                    }
                    if handle_clone.supports_shared_buffer() {
                        let _ = handle_clone.send_binary("stream", &chunk);
                    }
                }
            });
        })
        .run()
        .expect("Failed to run WebView");
}
