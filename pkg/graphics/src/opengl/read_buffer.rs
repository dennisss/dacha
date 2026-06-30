use common::errors::*;
use gl::types::{GLenum, GLint, GLsizei, GLuint};

use crate::opengl::texture::Texture;
use crate::opengl::window::WindowContext;


pub struct PixelReadBuffer {
    window_context: WindowContext,
    buffer_object: GLuint,
    size: usize,
}

// TODO: Drop handler.

impl PixelReadBuffer {
    pub fn new(
        mut window_context: WindowContext,
        size: usize
    ) -> Result<Self> {
        window_context.make_current();

        let mut buf = 0;
        unsafe {
            gl::GenBuffers(1, &mut buf);
            gl::BindBuffer(gl::PIXEL_PACK_BUFFER, buf);
            gl::BufferData(
                gl::PIXEL_PACK_BUFFER,
                size as isize,
                core::ptr::null(),
                gl::STREAM_READ,
            );
            gl::BindBuffer(gl::PIXEL_PACK_BUFFER, buf);
        }

        Ok(Self {
            window_context,
            buffer_object: buf,
            size,
        })
    }

    pub fn bind(&mut self) {
        unsafe {
            gl::BindBuffer(gl::PIXEL_PACK_BUFFER, self.buffer_object);
        }
    }

    pub fn unbind(&mut self) {
        unsafe {
            gl::BindBuffer(gl::PIXEL_PACK_BUFFER, 0);
        }
    }

    // TODO: Return a reference and do the y flipping with that.
    pub fn read(&mut self) -> Vec<u8> {
        let mut out = vec![];
        
        self.bind();
        unsafe {
            // NOTE: We assume that this will block until operations on the buffer are complete. 
            let ptr = gl::MapBuffer(gl::PIXEL_PACK_BUFFER, gl::READ_ONLY);
            assert!(ptr != core::ptr::null_mut());

            let data = core::slice::from_raw_parts_mut::<u8>(
                core::mem::transmute(ptr),
                self.size
            );

            out.extend_from_slice(data);

            gl::UnmapBuffer(gl::PIXEL_PACK_BUFFER);
        }
        self.unbind();

        out
    }
}
