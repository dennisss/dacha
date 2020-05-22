use crate::lighting::{LightSource, Material, MAX_LIGHTS};
use crate::util::*;
use common::async_std::fs::{read_to_string, File};
use common::async_std::io::Read;
use common::errors::*;
use gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use math::matrix::Matrix4f;
use std::ffi::CStr;
use std::ptr::null;

const MAX_ERROR_LENGTH: GLsizei = 2048;

pub struct Shader {
    pub program: GLuint,
    pub pos_attrib: GLuint,
    pub normal_attrib: Option<GLuint>,
    pub color_attrib: GLuint,

    // TODO: These are not attributes, they are uniform locations?
    pub uni_proj_attrib: GLuint,
    pub uni_modelview_attrib: GLuint,

    pub eyepos_attrib: Option<GLuint>,
}

impl Shader {
    pub async fn Default() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/flat.vertex.glsl",
            "pkg/graphics/shaders/flat.fragment.glsl",
        )
        .await
    }

    pub async fn Gouraud() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/gouraud.vertex.glsl",
            "pkg/graphics/shaders/gouraud.fragment.glsl",
        )
        .await
    }

    pub async fn Phong() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/phong.vertex.glsl",
            "pkg/graphics/shaders/phong.fragment.glsl",
        )
        .await
    }

    pub async fn load_files(vertex_path: &str, fragment_path: &str) -> Result<Self> {
        let vertex_src = read_to_string(vertex_path).await?;
        let fragment_src = read_to_string(fragment_path).await?;
        Self::load(vertex_src.as_ref(), fragment_src.as_ref())
    }

    // TODO: Clean up on Drop (also if only part of it succeeds, we should clean
    // up just that part).
    pub fn load(vertex_src: &str, fragment_str: &str) -> Result<Self> {
        unsafe {
            let vertex_shader = gl_create_shader(gl::VERTEX_SHADER, vertex_src)?;
            let fragment_shader = gl_create_shader(gl::FRAGMENT_SHADER, fragment_str)?;

            let program = gl::CreateProgram();
            gl::AttachShader(program, vertex_shader);
            gl::AttachShader(program, fragment_shader);

            //			gl::BindFragDataLocation(program, 0,
            // std::ffi::CString::new("fragColor").unwrap().as_ptr());

            gl::LinkProgram(program);

            // TODO: Copied from  https://github.com/brendanzab/gl-rs/blob/master/gl/examples/triangle.rs
            {
                let mut status = gl::FALSE as GLint;
                gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

                // Fail on error
                if status != (gl::TRUE as GLint) {
                    let mut len: GLint = 0;
                    gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
                    let mut buf = Vec::with_capacity(len as usize);
                    buf.set_len((len as usize) - 1); // subtract 1 to skip the trailing null character
                    gl::GetProgramInfoLog(
                        program,
                        len,
                        std::ptr::null_mut(),
                        buf.as_mut_ptr() as *mut GLchar,
                    );
                    panic!(
                        "{}",
                        std::str::from_utf8(&buf)
                            .ok()
                            .expect("ProgramInfoLog not valid utf8")
                    );
                }
            }

            let pos_attrib = gl_get_attrib(program, b"position\0").unwrap();
            let normal_attrib = gl_get_attrib(program, b"normal\0");
            let color_attrib = gl_get_attrib(program, b"color\0").expect("Missing color");
            // TODO: These are no longer uniform variables.
            let uni_modelview_attrib =
                gl_get_location(program, b"modelview\0").expect("Missing modelview");
            let uni_proj_attrib = gl_get_location(program, b"proj\0").expect("Missing proj");
            let eyepos_attrib = gl_get_location(program, b"eyePosition\0");

            Ok(Self {
                program,
                pos_attrib,
                normal_attrib,
                color_attrib,
                uni_proj_attrib,
                uni_modelview_attrib,
                eyepos_attrib,
            })
        }
    }

    /// Initialize the shader for the current vertex array object
    /// TODO: We must assert that all of these are specified for each object. It
    /// will crash if not?
    pub fn init(&self) {
        unsafe {
            gl::EnableVertexAttribArray(self.pos_attrib);
            gl::EnableVertexAttribArray(self.color_attrib);
            if let Some(attr) = self.normal_attrib {
                gl::EnableVertexAttribArray(attr);
            }
        }
    }

    pub fn set_lights(&self, lights: &[LightSource]) {
        if lights.len() > MAX_LIGHTS {
            panic!("Too many lights!");
        }

        let nlights_attr = match gl_get_attrib(self.program, b"nlights\0") {
            Some(a) => a,
            // Do nothing if the shader does not support lighting.
            None => {
                return;
            }
        };

        unsafe {
            gl::Uniform1i(nlights_attr as GLint, lights.len() as GLint);
        }

        for i in 0..lights.len() {
            let p_attr =
                gl_get_location(self.program, format!("lights[{}].position\0", i).as_bytes())
                    .unwrap();
            let a_attr =
                gl_get_location(self.program, format!("lights[{}].ambient\0", i).as_bytes())
                    .unwrap();
            let d_attr =
                gl_get_location(self.program, format!("lights[{}].diffuse\0", i).as_bytes())
                    .unwrap();
            let s_attr =
                gl_get_location(self.program, format!("lights[{}].specular\0", i).as_bytes())
                    .unwrap();

            gl_uniform_vec4(p_attr, &lights[i].position);
            gl_uniform_vec3(a_attr, &lights[i].ambient);
            gl_uniform_vec3(d_attr, &lights[i].diffuse);
            gl_uniform_vec3(s_attr, &lights[i].specular);
        }
    }

    pub fn set_material(&self, material: &Material) {
        let a_attr = gl_get_location(self.program, b"material.ambient\0").unwrap();
        let d_attr = gl_get_location(self.program, b"material.diffuse\0").unwrap();
        let s_attr = gl_get_location(self.program, b"material.specular\0").unwrap();
        let sh_attr = gl_get_location(self.program, b"material.shininess\0").unwrap();

        gl_uniform_vec3(a_attr, &material.ambient);
        gl_uniform_vec3(d_attr, &material.diffuse);
        gl_uniform_vec3(s_attr, &material.specular);
        unsafe {
            gl::Uniform1f(sh_attr as GLint, material.shininess);
        }
    }
}

fn gl_create_shader(typ: GLenum, src: &str) -> Result<GLuint> {
    let shader = unsafe { gl::CreateShader(typ) };
    // TODO: Check if this is correct for a utf-8 string
    let strings = [unsafe { std::mem::transmute(src.as_ptr()) }];
    let lengths = [src.len() as GLint];
    unsafe {
        gl::ShaderSource(shader, 1, strings.as_ptr(), lengths.as_ptr());
        gl::CompileShader(shader);
    }

    let mut status = 0;
    unsafe {
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
    }

    if status != (gl::TRUE as i32) {
        let mut length = MAX_ERROR_LENGTH;
        let mut error_log = vec![];
        error_log.resize(length as usize, 0);
        unsafe {
            gl::GetShaderInfoLog(shader, length, &mut length, &mut error_log[0]);
        }

        // TODO: Does this include the null terminator.
        let length = length as usize;
        if length > error_log.len() {
            return Err(err_msg("Invalid error length returned by OpenGL"));
        }

        return Err(format_err!(
            "Shader failed to compile: {}",
            std::str::from_utf8(unsafe { std::mem::transmute(&error_log[..length]) })?
        ));
    }

    Ok(shader)
}

fn gl_get_attrib(program: GLuint, name: &[u8]) -> Option<GLuint> {
    assert!(name.len() > 0 && *name.last().unwrap() == 0);
    let attr = unsafe {
        gl::GetAttribLocation(
            program,
            std::mem::transmute::<*const u8, *const i8>(name.as_ptr()),
        )
    };

    if attr < 0 {
        None
    } else {
        Some(attr as GLuint)
    }
}

fn gl_get_location(program: GLuint, name: &[u8]) -> Option<GLuint> {
    assert!(name.len() > 0 && *name.last().unwrap() == 0);
    let loc = unsafe { gl::GetUniformLocation(program, std::mem::transmute(name.as_ptr())) };

    if loc < 0 {
        None
    } else {
        Some(loc as GLuint)
    }
}
