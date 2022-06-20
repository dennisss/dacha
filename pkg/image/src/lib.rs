#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate byteorder;
extern crate math;
#[macro_use]
extern crate parsing;
extern crate reflection;

#[macro_use]
extern crate lazy_static;

use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

use common::errors::*;
use math::array::Array;
use math::geometry::bounding_box::BoundingBox;
use math::matrix::{Vector2f, VectorStatic};
use math::number::{Cast, Zero};

pub mod format;
pub mod open;
pub mod resize;

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
#[derive(Clone)]
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

// TODO: Move to a separate file given this is starting to become complicated.
// TODO: Implement a custom debugger for this.
#[derive(Clone, Debug)]
pub struct Color {
    data: VectorStatic<u8, typenum::U4>,
}

impl Color {
    pub fn zero() -> Self {
        Self {
            data: VectorStatic::zero(),
        }
    }

    pub fn hex(val: i32) -> Self {
        let val = val as u32;
        Self::rgb((val >> 16) as u8, (val >> 8) as u8, val as u8)
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            data: VectorStatic::from_slice(&[r, g, b, 255]),
        }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            data: VectorStatic::from_slice(&[r, g, b, a]),
        }
    }
}

impl core::str::FromStr for Color {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(err_msg("Empty color string"));
        }

        let mut chars = s.chars();

        if chars.next() == Some('#') {
            let mut digits = vec![];
            for c in chars {
                digits.push(
                    c.to_digit(16)
                        .ok_or_else(|| err_msg("Not a valid hex digit"))?,
                );
            }

            let mut rgb = vec![];
            if digits.len() == 3 {
                for d in digits {
                    rgb.push(((d as u8) << 4) | (d as u8));
                }
            } else if digits.len() == 6 {
                for d in digits.chunks_exact(2) {
                    rgb.push(((d[0] as u8) << 4) | (d[1] as u8));
                }
            } else {
                return Err(err_msg("Wrong number of hex digits"));
            }

            return Ok(Color::rgb(rgb[0], rgb[1], rgb[2]));
        }

        Err(err_msg("Unknown color string format"))
    }
}

impl core::ops::Deref for Color {
    type Target = VectorStatic<u8, typenum::U4>;

    fn deref(&self) -> &Self::Target {
        // TODO: Limit to only the supported channels?
        &self.data
    }
}

impl core::ops::DerefMut for Color {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl core::convert::From<VectorStatic<u8, typenum::U4>> for Color {
    fn from(data: VectorStatic<u8, typenum::U4>) -> Self {
        Self { data }
    }
}

impl Image<u8> {
    pub fn clear_white(&mut self) {
        for i in self.array.data.iter_mut() {
            *i = 0xff;
        }
    }

    pub fn set(&mut self, y: usize, x: usize, color: &Color) {
        let start = self.channels() * (y * self.width() + x);

        for i in 0..self.channels() {
            self.array[start + i] = color[i];
        }
    }

    pub fn get(&self, y: usize, x: usize) -> Color {
        let mut color = Color::zero();
        for i in 0..self.channels() {
            color[i] = self[(y, x, i)];
        }
        color
    }
}

impl<T: Cast<f32> + Default + Copy> Image<T> {
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
        T: Cast<f32>,
        f32: Cast<T>,
    {
        assert_eq!(self.colorspace, Colorspace::RGB);

        let mut data = Vec::<T>::new();
        data.reserve(self.height() * self.width());

        for i in 0..self.height() {
            for j in 0..self.width() {
                let r: f32 = self.array[&[i, j, 0][..]].cast();
                let g: f32 = self.array[&[i, j, 1][..]].cast();
                let b: f32 = self.array[&[i, j, 2][..]].cast();
                data.push((0.299 * r + 0.587 * g + 0.114 * b).cast());
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

impl<T: Clone, Y: Copy + Cast<usize>> std::ops::Index<(Y, Y, Y)> for Image<T> {
    type Output = T;
    fn index(&self, index: (Y, Y, Y)) -> &T {
        self.array.index(&[index.0, index.1, index.2] as &[Y])
    }
}

impl<T: Clone, Y: Copy + Cast<usize>> std::ops::IndexMut<(Y, Y, Y)> for Image<T> {
    fn index_mut(&mut self, index: (Y, Y, Y)) -> &mut T {
        self.array.index_mut(&[index.0, index.1, index.2] as &[Y])
    }
}
