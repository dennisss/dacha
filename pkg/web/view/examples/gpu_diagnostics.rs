use webview::WebViewBuilder;

fn main() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>GPU Diagnostics</title>
        </head>
        <body style="margin: 0; padding: 0; overflow: hidden; background-color: white;">
            <div style="padding: 20px; font-family: sans-serif;">
                <h2>Redirecting to webkit://gpu...</h2>
                <p>If you are not redirected automatically, <a href="webkit://gpu">click here</a>.</p>
            </div>
            <script>
                // Attempt to navigate the webview to the internal GPU diagnostics page
                window.location.replace("webkit://gpu/stdout");
            </script>
        </body>
        </html>
    "#;

    WebViewBuilder::new("GPU Diagnostics", 1024, 768)
        .with_user_data_dir(std::env::temp_dir().join("webview_diagnostics").to_str().unwrap())
        .load_html(html)
        .run()
        .expect("Failed to run WebView");
}
