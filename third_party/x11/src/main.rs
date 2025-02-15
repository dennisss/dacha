use base_error::*;

fn main() -> Result<()> {
    let display = x11::Display::open_default()?;

    let root_window = display.root_window()?;

    let sub_windows = root_window.client_list()?;

    for window in sub_windows {
        let attrs = window.attrs()?;

        println!(
            "- {:?} : (Pid: {:?}, W: {}, H: {})",
            window.name()?,
            window.pid()?,
            attrs.width,
            attrs.height
        );
    }

    Ok(())
}
