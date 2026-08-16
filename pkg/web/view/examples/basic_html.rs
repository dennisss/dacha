#![windows_subsystem = "windows"]

use webview::WebViewBuilder;

fn main() {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>Basic HTML Demo</title>
</head>
<body>
    <h1>Minimal WebView</h1>
    <button onclick="document.getElementById('output').textContent = 'Button Clicked!'">Click Me</button>
    <div id="output"></div>
</body>
</html>
"#;

    WebViewBuilder::new("Basic HTML Example", 800, 600)
        .with_user_data_dir(std::env::temp_dir().join("webview_example_data").to_str().unwrap())
        .load_html(html)
        .run()
        .expect("Failed to run WebView");
}
