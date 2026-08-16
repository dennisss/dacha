# Web View Library

This is a library for embedding web views (HTML + Javascript) in a Rust application. It supports Windows (WebView2), macOS (WKWebView), Linux (libwebkit2gtk) by using a browser runtime already installed on your computer.

The general 'web <> rust' API we provide is the following:

- Rust opens the webview using the first page's HTML provided.
    - TODO: Maybe just have it navigate to URL and we can just pull the payload from on_request.
- Rust can use the `on_request()` and `send_response()` functions to process unary HTTP GET requests from the client.
    - This is the main efficient way to transfer binary data to the HTML/JS client.
    - Response streaming not supported since it is hard to guarantee low latency without buffering across all platforms.
- Rust can use `on_message()` and `post_message()` to transfer strings between the JS and Rust code.
    - This is a bidirectional interface but is only efficient for small string/json data.

Note that all communications are done via private IPC (no open TCP/UDP ports) so can be considered secure and requests coming into the webview code can be trusted to be from the UI (and not some other program on the computer trying to send to the same port).

## Dependencies

The following dependencies are required to run the software:

- **Windows 10+** : Should be installed by defualt. If not, install WebView2 from [here](https://developer.microsoft.com/en-us/Microsoft-edge/webview2).
- **Linux** : You need 'libwebkit2gtk' installed (any recent version should work since we dynamically try finding a library file):
    - For Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-0`
- **macOS** : No dependencies


## Developing

There are no additional webview dependencies required for compiling software but you may need  with the Webview but you probably need extra Rust targets installed for cross compilation.

e.g. windows on linux compilation (`msvc` is recommended single it statically links in ):

```
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
cargo xwin build --target x86_64-pc-windows-msvc --release --bin some_webview_app
```
