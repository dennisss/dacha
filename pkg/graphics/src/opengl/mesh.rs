use core::ptr::null;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use common::async_std::path::Path;
use common::errors::*;
use gl::types::{GLint, GLuint};
use math::matrix::Vector3f;

use crate::opengl::drawable::Drawable;
use crate::opengl::object::Object;
use crate::opengl::shader::Shader;
use crate::opengl::util::{gl_face_buffer, gl_vertex_buffer_vec3, GLBuffer};
use crate::opengl::window::Window;
use crate::transform::{Camera, Transform};

pub type Face = [GLuint; 3];

/// Drawing of generic triangle based meshes.
pub struct Mesh {
    object: Object,
    index_vbo: GLBuffer,
    nindices: usize,
}

// TODO: We don't want to expose all methods like set_vertex_positions?
impl_deref!(Mesh::object as Object);

// TODO: Do we need to use glUseProgram before using the
impl Mesh {
    pub async fn read(path: &str, shader: Rc<Shader>) -> Result<Self> {
        let path = Path::new(path);
        let ext = path
            .extension()
            .map(|s| s.to_str().unwrap())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext {
            //			"stl" => {
            //
            //			},
            _ => Err(format_err!("Unknown mesh format with extension: .{}", ext)),
        }

        /*
            if(strcmp(ext, "smf") == 0){
                return read_smf(filename, shader);
            }
            else if(strcmp(ext, "stl") == 0) {
                return read_stl(filename, shader);
            }
        //	else if(strcmp(ext, "txt") == 0){
        //		return read_patch(filename, shader);
        //	}

            cerr << "Unknown mesh format: " << ext << endl;
            return NULL;
            */
    }

    pub fn from(
        vertices: &[Vector3f],
        faces: &[Face],
        normals: &[Vector3f],
        shader: Rc<Shader>,
    ) -> Self {
        let mut object = Object::new(shader); // < Will bind the VAO

        object.set_vertex_positions(vertices);

        // TODO: Verify that all faces have in-range indices

        // When no normals are specified, we will generate normals by averaging
        // all faces connected to each vertex.
        let mut normal_buffer = vec![];
        let mut normals = normals;
        if normals.len() == 0 {
            normal_buffer.resize(vertices.len(), Vector3f::zero());
            for face in faces {
                let p0 = &vertices[face[0] as usize];
                let p1 = &vertices[face[1] as usize];
                let p2 = &vertices[face[2] as usize];
                let n = (p1 - p0).cross(&(p2 - p0));

                for idx in face {
                    normal_buffer[*idx as usize] += &n;
                }
            }

            normals = &normal_buffer;
        }

        object.set_vertex_normals(normals);

        let index_vbo = gl_face_buffer(faces);

        Self {
            object,
            index_vbo,
            nindices: 3 * faces.len(),
        }
    }
}

impl Drawable for Mesh {
    fn draw(&self, camera: &Camera, prev: &Transform) {
        self.object.draw(camera, prev);
        unsafe {
            gl::DrawElements(
                gl::TRIANGLES,
                self.nindices as i32,
                gl::UNSIGNED_INT,
                null(),
            );
        }
    }
}
