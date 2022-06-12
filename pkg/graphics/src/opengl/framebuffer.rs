use common::errors::*;
use gl::types::{GLenum, GLint, GLsizei, GLuint};

pub struct FrameBuffer {
    frame_buffer_object: GLuint,
    color_texture_object: GLuint,
    depth_render_buffer_object: GLuint,
}

impl Drop for FrameBuffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.frame_buffer_object);
            gl::DeleteTextures(1, &self.color_texture_object);
            gl::DeleteRenderbuffers(1, &self.depth_render_buffer_object);
        }
    }
}

impl FrameBuffer {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        let mut frame_buffer_object = 0;
        let mut color_texture_object = 0;
        let mut depth_render_buffer_object = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut frame_buffer_object);
            gl::BindFramebuffer(gl::FRAMEBUFFER, frame_buffer_object);

            // Create the color texture which will store the RGB output of the framebuffer.
            gl::GenTextures(1, &mut color_texture_object);
            gl::BindTexture(gl::TEXTURE_2D, color_texture_object);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as GLint,
                width as GLint,
                height as GLint,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                core::ptr::null(),
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);

            // Attach color texture to frame buffer.
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                color_texture_object,
                0,
            );

            // Create render buffer for depth and stencil data (doesn't need to be a texture
            // given we will never display it).
            gl::GenRenderbuffers(1, &mut depth_render_buffer_object);
            gl::BindRenderbuffer(gl::RENDERBUFFER, depth_render_buffer_object);
            gl::RenderbufferStorage(
                gl::RENDERBUFFER,
                gl::DEPTH24_STENCIL8,
                width as GLint,
                height as GLint,
            );
            gl::BindRenderbuffer(gl::RENDERBUFFER, 0);

            // Attach render buffer to frame buffer.
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::DEPTH_STENCIL_ATTACHMENT,
                gl::RENDERBUFFER,
                depth_render_buffer_object,
            );

            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                return Err(err_msg("Failed to instantiate frame buffer"));
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        Ok(Self {
            frame_buffer_object,
            color_texture_object,
            depth_render_buffer_object,
        })
    }

    pub fn draw_context<T, F: FnOnce() -> T>(&self, f: F) -> T {
        unsafe { gl::BindFramebuffer(gl::FRAMEBUFFER, self.frame_buffer_object) };
        let ret = f();
        unsafe { gl::BindFramebuffer(gl::FRAMEBUFFER, 0) };
        ret
    }

    pub fn bind(&self) {
        unsafe { gl::BindTexture(gl::TEXTURE_2D, self.color_texture_object) };
    }
}
