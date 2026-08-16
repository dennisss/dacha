use webview::show_error_dialog;

fn main() {
    show_error_dialog("Initialization Error", "The application failed to start correctly. Please check the logs.");
    println!("Displayed error dialog and continuing...");
}
