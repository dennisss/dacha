#[macro_use]
extern crate macros;

use std::time::Instant;

use base_error::*;
use file::project_path;
use image::format::jpeg::encoder::JPEGEncoder;
use image::{Color, Image};

/*
X11 reference is here:
- https://www.x.org/releases/X11R7.6/doc/libX11/specs/libX11/libX11.html#id2640964

*/

fn get_screenshot() -> Result<Image<u8>> {
    let display = x11::Display::open_default()?;

    let root_window = display.root_window()?;

    let sub_windows = root_window.client_list()?;

    println!("Found Windows: {}", sub_windows.len());
    for window in sub_windows {
        println!("- {:?} (pid: {:?})", window.name()?, window.pid()?);

        // println!("{:?}", window.list_properties()?);
    }

    let attrs = root_window.attrs()?;
    println!("{:?}", attrs);

    let start = Instant::now();

    let ximage = root_window.get_full_image(&attrs)?;

    let end = Instant::now();

    println!("Capture takes: {:?}", end - start);

    println!("{:?}", ximage);

    println!("LSB FIRST: {}", x11::bindings::LSBFirst);

    /*
    Data should be 32-bit
    */

    // TODO: Check aligned.

    let data = unsafe {
        core::slice::from_raw_parts(
            core::mem::transmute::<_, *const u32>(ximage.data),
            (ximage.width * ximage.height) as usize,
        )
    };

    let mut out = Image::<u8>::zero(
        ximage.height as usize,
        ximage.width as usize,
        image::Colorspace::RGB,
    );

    for y in 0..out.height() {
        for x in 0..out.width() {
            let i = y * out.width() + x;

            let v = data[i];

            let r = ((v & (ximage.red_mask as u32)) >> 16) as u8;
            let g = ((v & (ximage.green_mask as u32)) >> 8) as u8;
            let b = ((v & (ximage.blue_mask as u32)) >> 0) as u8;

            out.set(y, x, &Color::rgb(r, g, b));
        }
    }

    Ok(out)
}

#[executor_main]
async fn main() -> Result<()> {
    let mut image = get_screenshot()?;

    let encoder = JPEGEncoder::new(80);
    let mut data = vec![];
    encoder.encode(&image, &mut data)?;
    file::write(project_path!("screenshot.jpeg"), &data).await?;

    Ok(())
}
