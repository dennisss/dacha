
extern crate core;
extern crate math;
extern crate format;
extern crate minifb;

use format::errors::*;
use format::image::jpeg::JPEG;
use format::image::bitmap::Bitmap;
use format::image::Image;
use math::array::{KernelEdgeMode, Array};

use minifb::{Key, WindowOptions, Window};


fn main() -> Result<()> {
	// let jpeg = JPEG::open("/home/dennis/workspace/dacha/testdata/lena.jpg")?;
	let bmp = Bitmap::open("/home/dennis/workspace/dacha/testdata/valve.bmp")?;
	// display(&bmp.image.to_grayscale());

	let edges = CannyEdgeDetector::detect(&bmp.image);
	
	display(&edges);

	Ok(())

	/*
	let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];

	let mut window = Window::new("Test - ESC to exit",
								 WIDTH,
								 HEIGHT,
								 WindowOptions::default()).unwrap_or_else(|e| {
		panic!("{}", e);
	});

	while window.is_open() && !window.is_key_down(Key::Escape) {
		for i in buffer.iter_mut() {
			*i = 0; // write something more funny here!
		}

		// We unwrap here as we want this code to exit if it fails. Real applications may want to handle this in a different way
		window.update_with_buffer(&buffer).unwrap();
	}
	*/
}