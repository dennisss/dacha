use std::sync::{Arc, Mutex, Weak};

use gl::types::{GLenum, GLint, GLsizei, GLuint};
use image::Image;

use crate::window::Window;

pub struct Texture {
    object: GLuint,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { gl::DeleteTextures(1, &self.object) };
    }
}

impl Texture {
    /// Creates a new texture and stores an image into it.
    ///
    /// The bottom-left corner will be at (0,0) in texture coordinates and the
    /// top-right will be at (1,1).
    pub fn new(image: &Image<u8>) -> Self {
        let mut object = 0;

        // TODO: Check the colorspace.

        unsafe {
            gl::GenTextures(1, &mut object);
            gl::BindTexture(gl::TEXTURE_2D, object);

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as GLint);

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as GLint);

            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_MIN_FILTER,
                gl::LINEAR_MIPMAP_LINEAR as GLint,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);

            // NOTE: Even when RGB is specified as the format, 4-byte alignment of each
            // input pixel is expected. We disable that here. https://www.reddit.com/r/opengl/comments/8qk5ce/anything_special_required_for_non_power_of_2/
            //
            // Alternatively we could convert the image to RGBA and then feed that to
            // opengl.
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as GLint,
                image.width() as GLsizei,
                image.height() as GLsizei,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                core::mem::transmute(image.array.data.as_ptr()),
            );

            // Reset override.
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 4);

            gl::GenerateMipmap(gl::TEXTURE_2D);
        }

        Self { object }
    }

    /// Binds this texture as the active 2D texture so that it can be used in
    /// future drawing calls.
    pub fn bind(&self) {
        unsafe { gl::BindTexture(gl::TEXTURE_2D, self.object) };
    }
}
