#![windows_subsystem = "windows"]

use webview::WebViewBuilder;

const INDEX_HTML: &str = r##"
<!DOCTYPE html>
<html>
<body>
    <h1>Local Assets</h1>
    <img src="webview://localhost/test.svg" alt="Test Image">
</body>
</html>
"##;

const TEST_SVG: &str = r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg"><rect width="100" height="100" fill="blue"/></svg>"##;

fn main() {
    WebViewBuilder::new("Local Assets Example", 800, 600)
        .with_user_data_dir(std::env::temp_dir().join("webview_example_data").to_str().unwrap())
        .load_html(INDEX_HTML)
        .on_request(|handle, request_id, uri| {
            if uri.ends_with("test.svg") {
                let _ = handle.send_response(&request_id, 200, "image/svg+xml", TEST_SVG.as_bytes());
            } else {
                let _ = handle.send_response(&request_id, 404, "text/plain", b"Not Found");
            }
        })
        .run()
        .expect("Failed to run WebView");
}
