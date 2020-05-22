extern crate image;
extern crate common;

use common::errors::*;
use image::format::bitmap::Bitmap;

fn main() -> Result<()> {
	let bmp = Bitmap::open("/home/dennis/workspace/dacha/testdata/valve.bmp")?;

	bmp.image.show()?;

	let resized = bmp.image.resize(400, 400);

	resized.show()?;


	Ok(())
}