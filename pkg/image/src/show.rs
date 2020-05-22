use common::errors::*;
use crate::Image;

impl Image<u8> {
	pub fn show(&self) -> Result<()> {
		let mut window_options = minifb::WindowOptions::default();

		let mut window = minifb::Window::new(
			"Image",  self.width(), self.height(), window_options).unwrap();

		// 60 FPS
		window.limit_update_rate(Some(std::time::Duration::from_micros(16666)));

		let mut data = vec![0u32; self.array.data.len()];

		for (i, color) in self.array.flat().chunks_exact(self.channels()).enumerate() {
			data[i] = ((color[0] as u32) << 16) | ((color[1] as u32) << 8)
				| (color[2] as u32);
		}



		while window.is_open() {
			if window.is_key_pressed(minifb::Key::Escape, minifb::KeyRepeat::No) {
				break;
			}

			// TODO: Only update once as the image will be static.
			window.update_with_buffer(&data, self.width(), self.height())?;
		}

		Ok(())
	}


}