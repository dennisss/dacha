use std::ops::{Add, Mul, Div, Sub, AddAssign, SubAssign};
use num_traits::{Num, NumCast, AsPrimitive};
use math::array::Array;

// Abstract over an Array of unknown type

// Must be able to convert an Image to another type and still 

// Will always be 3D
pub struct Image<T> {
	// This will always have the shape: height x width x channels 
	array: Array<T>, // TODO: Will need to separate images in different formats?
}

impl<T: AsPrimitive<f32> + NumCast + Default + Copy> Image<T> {
	// TODO: Need image to always be 3d to avoid this having to do a bounds check.

	pub fn new(array: Array<T>) -> Image<T> {
		Image { array }
	}

	pub fn array(&self) -> &Array<T> {
		&self.array
	}

	pub fn height(&self) -> usize {
		self.array.shape[0]
	}
	pub fn width(&self) -> usize {
		self.array.shape[1]
	}
	pub fn channels(&self) -> usize {
		if self.array.shape.len() == 2 {
			return 1;
		}

		// TODO: any dimension beyond the end can be trivially 0 (and we should automatically trim any trailing dimensions like that?)
		self.array.shape[2]
	}

	/*
	pub fn extract_channel(&self, idx: usize) -> Image<T> {
		Image {
			array: self.array.slice(&[ArrayIndex::All, ArrayIndex::All, ArrayIndex::One(idx)])
		}
	}
	*/

	// Converts an RGB image to a single channel grayscale one.
	// Equivalent to a YUV taking only the Y channel.
	pub fn to_grayscale(&self) -> Image<T> where f32: AsPrimitive<T> {
		let mut data = Vec::<T>::new();
		data.reserve(self.height() * self.width());

		for i in 0..self.height() {
			for j in 0..self.width() {
				let r: f32 = self.array[&[i, j, 0][..]].as_();
				let g: f32 = self.array[&[i, j, 1][..]].as_();
				let b: f32 = self.array[&[i, j, 2][..]].as_();
				data.push((0.299*r + 0.587*g + 0.114*b).as_());
			}
		}
		
		Image {
			array: Array {
				shape: vec![self.height(), self.width(), 1],
				data
			}
		}
	}

	// For images, we can perform nice resizing of stuff
}

pub mod bitmap;
pub mod jpeg;