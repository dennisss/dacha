use gl::types::{GLenum, GLint, GLsizei, GLuint};
use image::{Colorspace, Image};

use crate::opengl::window::*;

// The internal fields of this are also re-used by the FrameBuffer struct.
pub struct Texture {
    pub(super) context: WindowContext,
    pub(super) object: GLuint,
}

impl Drop for Texture {
    fn drop(&mut self) {
        self.context.make_current();
        unsafe { gl::DeleteTextures(1, &self.object) };
    }
}

// TODO: Possibly consider allowing usage of PBOs to asynchronously start
// uploading data to OpenGL.

impl Texture {
    /// Creates a new texture and stores an image into it.
    ///
    /// The bottom-left corner will be at (0,0) in texture coordinates and the
    /// top-right will be at (1,1).
    pub fn new(mut context: WindowContext, image: &Image<u8>) -> Self {
        // Challenge: Won't have

        context.make_current();

        let mut object = 0;

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

            let format = match image.colorspace {
                Colorspace::RGB => gl::RGB,
                Colorspace::RGBA => gl::RGBA,
                _ => todo!(),
            };

            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                format as GLint,
                image.width() as GLsizei,
                image.height() as GLsizei,
                0,
                format,
                gl::UNSIGNED_BYTE,
                core::mem::transmute(image.array.data.as_ptr()),
            );

            // Reset override.
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 4);

            gl::GenerateMipmap(gl::TEXTURE_2D);
        }

        Self { context, object }
    }

    /// Binds this texture as the active 2D texture so that it can be used in
    /// future drawing calls.
    pub fn bind(&self) {
        unsafe { gl::BindTexture(gl::TEXTURE_2D, self.object) };
    }
}
