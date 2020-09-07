extern crate common;
#[macro_use]
extern crate macros;
extern crate byteorder;
extern crate math;
extern crate minifb;
extern crate num_traits;
#[macro_use]
extern crate parsing;
extern crate reflection;

use math::array::Array;
use math::geometry::bounding_box::BoundingBox;
use math::matrix::Vector2f;
use num_traits::{AsPrimitive, Num, NumCast, Zero};
use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

pub mod format;
pub mod resize;
pub mod show;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Colorspace {
    RGB,
    RGBA,
    Grayscale,
}

impl Colorspace {
    pub fn channels(&self) -> usize {
        use Colorspace::*;
        match self {
            RGB => 3,
            RGBA => 4,
            Grayscale => 1,
        }
    }
}

// Abstract over an Array of unknown type

// Must be able to convert an Image to another type and still

/// An image is a pixel buffer containing color values.
/// It is implemented as an Array with generic type, but with known rank of 3.
/// The shape is of the form: (height, width, num_channels)
pub struct Image<T> {
    /// This will always have the shape: height x width x channels
    pub array: Array<T>, // TODO: Will need to separate images in different formats?
    pub colorspace: Colorspace,
}

impl<T: Zero + Clone> Image<T> {
    pub fn zero(height: usize, width: usize, colorspace: Colorspace) -> Self {
        Image {
            array: Array {
                data: vec![T::zero(); height * width * colorspace.channels()],
                shape: vec![height, width, colorspace.channels()],
            },
            colorspace,
        }
    }
}

pub type Color = math::matrix::Vector<u8, math::matrix::Dynamic>;

impl Image<u8> {
    pub fn clear_white(&mut self) {
        for i in self.array.data.iter_mut() {
            *i = 0xff;
        }
    }

    pub fn set(&mut self, y: usize, x: usize, color: &Color) {
        assert_eq!(color.len(), self.channels());
        for i in 0..self.channels() {
            self[(y, x, i)] = color[i];
        }
    }

    pub fn get(&self, y: usize, x: usize) -> Color {
        let mut color = Color::zero_with_shape(self.channels(), 1);
        for i in 0..self.channels() {
            color[i] = self[(y, x, i)];
        }
        color
    }
}

impl<T: AsPrimitive<f32> + NumCast + Default + Copy> Image<T> {
    // TODO: Need image to always be 3d to avoid this having to do a bounds check.

    pub fn wrap(array: Array<T>, colorspace: Colorspace) -> Image<T> {
        assert_eq!(array.shape.len(), 3);
        assert_eq!(colorspace.channels(), array.shape[2]);
        Image { array, colorspace }
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

        // TODO: any dimension beyond the end can be trivially 0 (and we should
        // automatically trim any trailing dimensions like that?)
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
    pub fn to_grayscale(&self) -> Image<T>
    where
        f32: AsPrimitive<T>,
    {
        assert_eq!(self.colorspace, Colorspace::RGB);

        let mut data = Vec::<T>::new();
        data.reserve(self.height() * self.width());

        for i in 0..self.height() {
            for j in 0..self.width() {
                let r: f32 = self.array[&[i, j, 0][..]].as_();
                let g: f32 = self.array[&[i, j, 1][..]].as_();
                let b: f32 = self.array[&[i, j, 2][..]].as_();
                data.push((0.299 * r + 0.587 * g + 0.114 * b).as_());
            }
        }

        Image {
            array: Array {
                shape: vec![self.height(), self.width(), 1],
                data,
            },
            colorspace: Colorspace::Grayscale,
        }
    }

    pub fn bbox(&self) -> BoundingBox<typenum::U2> {
        BoundingBox {
            min: Vector2f::from_slice(&[0.0, 0.0]),
            max: Vector2f::from_slice(&[(self.width() - 1) as f32, (self.height() - 1) as f32]),
        }
    }

    // For images, we can perform nice resizing of stuff
}

impl<T: Clone, Y: AsPrimitive<usize>> std::ops::Index<(Y, Y, Y)> for Image<T> {
    type Output = T;
    fn index(&self, index: (Y, Y, Y)) -> &T {
        self.array.index(&[index.0, index.1, index.2] as &[Y])
    }
}

impl<T: Clone, Y: AsPrimitive<usize>> std::ops::IndexMut<(Y, Y, Y)> for Image<T> {
    fn index_mut(&mut self, index: (Y, Y, Y)) -> &mut T {
        self.array.index_mut(&[index.0, index.1, index.2] as &[Y])
    }
}
