use core::ptr::null;
use std::collections::HashMap;
use std::ffi::CStr;

use common::errors::*;
use common::failure::ResultExt;
use common::hash::SumHasherBuilder;
use file::read_to_string;
use gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use math::matrix::Matrix4f;

use crate::lighting::{LightSource, Material, MAX_LIGHTS};
use crate::opengl::util::*;
use crate::opengl::window::*;

const MAX_ERROR_LENGTH: GLsizei = 2048;

const SIMPLE_VERTEX_SHADER: &'static str = r#"
#version 330

uniform mat4 u_proj;
uniform mat4 u_modelview;

in vec3 v_position;
in vec3 v_color;
in float v_alpha;
in vec2 v_tex_coord;

out vec2 f_tex_coord;
out vec4 f_color;

void main() {
    f_color = vec4(v_color, v_alpha);
    f_tex_coord = v_tex_coord;
	gl_Position = u_proj * u_modelview * vec4(v_position, 1.0);
}
"#;

const SIMPLE_FRAGMENT_SHADER: &'static str = r#"
#version 330

in vec2 f_tex_coord;
in vec4 f_color;

uniform sampler2D u_texture;

out vec4 frag_color;

void main() {
	frag_color = texture(u_texture, f_tex_coord) * f_color;
}
"#;

enum_def!(
    /// All known per-vertex attributes used by any shader.
    /// The string value of each case should match the name of the 'in' variable in the shader.
    ShaderAttributeId str =>
        VertexPosition = "v_position",
        VertexColor = "v_color",
        VertexAlpha = "v_alpha",
        VertexTextureCoordinate = "v_tex_coord",
        VertexNormal = "v_normal"
);

enum_def!(ShaderUniformId str =>
    ProjectionMatrix = "u_proj",
    ModelViewMatrix = "u_modelview",
    CameraPosition = "u_camera_position",
    Texture = "u_texture"
);

pub struct ShaderSource {
    vertex_src: String,
    fragment_src: String,
}

impl ShaderSource {
    pub async fn simple() -> Result<Self> {
        Ok(Self {
            vertex_src: SIMPLE_VERTEX_SHADER.to_string(),
            fragment_src: SIMPLE_FRAGMENT_SHADER.to_string(),
        })
    }

    pub async fn flat() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/flat.vertex.glsl",
            "pkg/graphics/shaders/flat.fragment.glsl",
        )
        .await
    }

    pub async fn flat_texture() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/flat_texture.vertex.glsl",
            "pkg/graphics/shaders/flat_texture.fragment.glsl",
        )
        .await
    }

    pub async fn gouraud() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/gouraud.vertex.glsl",
            "pkg/graphics/shaders/gouraud.fragment.glsl",
        )
        .await
    }

    pub async fn phong() -> Result<Self> {
        Self::load_files(
            "pkg/graphics/shaders/phong.vertex.glsl",
            "pkg/graphics/shaders/phong.fragment.glsl",
        )
        .await
    }

    async fn load_files(vertex_path: &str, fragment_path: &str) -> Result<Self> {
        let vertex_src = read_to_string(vertex_path).await?;
        let fragment_src = read_to_string(fragment_path).await?;

        Ok(Self {
            vertex_src,
            fragment_src,
        })
    }

    pub fn compile(&self, window: &mut Window) -> Result<Shader> {
        Shader::load(&self.vertex_src, &self.fragment_src, window)
    }
}

pub struct Shader {
    context: WindowContext,
    /// TODO: Delete this on drop as well as the inner shader objects.
    /// ^ Also need to stop using the shader if it is currently in use.
    pub program: GLuint,

    pub attrs: HashMap<ShaderAttributeId, GLuint, SumHasherBuilder>,
    pub uniforms: HashMap<ShaderUniformId, GLuint, SumHasherBuilder>,
}

impl Shader {
    // TODO: Clean up on Drop (also if only part of it succeeds, we should clean
    // up just that part).
    pub fn load(vertex_src: &str, fragment_str: &str, window: &mut Window) -> Result<Self> {
        Self::load_program(
            &[
                (gl::VERTEX_SHADER, vertex_src),
                (gl::FRAGMENT_SHADER, fragment_str),
            ],
            window,
        )
    }

    pub fn load_compute(src: &str, window: &mut Window) -> Result<Self> {
        Self::load_program(&[(gl::COMPUTE_SHADER, src)], window)
    }

    fn load_program(parts: &[(GLenum, &str)], window: &mut Window) -> Result<Self> {
        let mut context = window.context();
        context.make_current();

        unsafe {
            let program = gl::CreateProgram();

            for (typ, src) in parts.iter().cloned() {
                let shader = gl_create_shader(typ, src)?;
                gl::AttachShader(program, shader);
            }

            //			gl::BindFragDataLocation(program, 0,
            // std::ffi::CString::new("fragColor").unwrap().as_ptr());

            gl::LinkProgram(program);

            // Verify that the programs have compiled successfully. If not we will print the
            // error. TODO: Copied from  https://github.com/brendanzab/gl-rs/blob/master/gl/examples/triangle.rs
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

                    return Err(format_err!(
                        "Shader compilation failed: {}",
                        std::str::from_utf8(&buf).with_context(|e| format_err!(
                            "Shader ProgramInfoLog not valid utf8: {:?}",
                            e
                        ))?
                    ));
                }
            }

            let mut attrs = HashMap::with_hasher(SumHasherBuilder::default());
            let mut uniforms = HashMap::with_hasher(SumHasherBuilder::default());

            let mut num_attrs = 0;
            gl::GetProgramiv(program, gl::ACTIVE_ATTRIBUTES, &mut num_attrs);

            for i in 0..num_attrs {
                let mut name_buf = [0u8; 256];
                let mut name_length = 0;
                let mut size = 0;
                let mut typ = 0;

                gl::GetActiveAttrib(
                    program,
                    i as GLuint,
                    name_buf.len() as GLsizei,
                    &mut name_length,
                    &mut size,
                    &mut typ,
                    name_buf.as_mut_ptr() as *mut GLchar,
                );

                if name_length <= 0 || ((name_length as usize) + 1) >= name_buf.len() {
                    return Err(err_msg("Attribute name overflowed buffer"));
                }

                let name = core::str::from_utf8(&name_buf[0..(name_length as usize)])?;

                let id = ShaderAttributeId::from_value(name)
                    .map_err(|_| format_err!("Unknown shader attribute named: {}", name))?;

                let location =
                    gl_get_attrib(program, &name_buf[0..(name_length as usize + 1)]).unwrap();

                attrs.insert(id, location);
            }

            let mut num_uniforms = 0;
            gl::GetProgramiv(program, gl::ACTIVE_UNIFORMS, &mut num_uniforms);

            for i in 0..num_uniforms {
                let mut name_buf = [0u8; 256];
                let mut name_length = 0;
                let mut size = 0;
                let mut typ = 0;

                gl::GetActiveUniform(
                    program,
                    i as GLuint,
                    name_buf.len() as GLsizei,
                    &mut name_length,
                    &mut size,
                    &mut typ,
                    name_buf.as_mut_ptr() as *mut GLchar,
                );

                if name_length <= 0 || ((name_length as usize) + 1) >= name_buf.len() {
                    return Err(err_msg("Uniform name overflowed buffer"));
                }

                let name = core::str::from_utf8(&name_buf[0..(name_length as usize)])?;

                let id = ShaderUniformId::from_value(name)
                    .map_err(|_| format_err!("Unknown shader uniform named: {}", name))?;

                let location =
                    gl_get_location(program, &name_buf[0..(name_length as usize + 1)]).unwrap();

                uniforms.insert(id, location);
            }

            Ok(Self {
                context,
                program,
                attrs,
                uniforms,
            })
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
        gl::GetAttribLocation(program, core::mem::transmute::<*const u8, _>(name.as_ptr()))
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
