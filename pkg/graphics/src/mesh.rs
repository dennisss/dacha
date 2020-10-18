use crate::drawable::{Drawable, Object};
use crate::shader::Shader;
use crate::transform::{Camera, Transform};
use crate::util::{gl_face_buffer, gl_vertex_buffer_vec3, GLBuffer};
use common::async_std::path::Path;
use common::errors::*;
use gl::types::{GLint, GLuint};
use math::matrix::Vector3f;
use std::ptr::null;
use std::sync::Arc;

pub type Face = [GLuint; 3];

/// Drawing of generic triangle based meshes.
pub struct Mesh {
    object: Object,

    pos_vbo: GLBuffer,
    normal_vbo: Option<GLBuffer>,
    color_vbo: Option<GLBuffer>,
    index_vbo: GLBuffer,
    nindices: usize,
}

impl_deref!(Mesh::object as Object);

// TODO: Do we need to use glUseProgram before using the
impl Mesh {
    pub async fn read(path: &str, shader: Arc<Shader>) -> Result<Self> {
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
        colors: &[Vector3f],
        faces: &[Face],
        normals: &[Vector3f],
        shader: Arc<Shader>,
    ) -> Self {
        // Setup shader
        // TODO: Do I need to reset the program in the draw?
        unsafe {
            gl::UseProgram(shader.program);
        }
        // TODO: Setup uniforms

        let pos_vbo = gl_vertex_buffer_vec3(shader.pos_attrib, vertices);

        let color_vbo =
			// If no colors are provided, we will not bind any value to the
			// attribute and will instead assume that the material is handling
			// all of the coloring
			if colors.len() == 0 { None }
			else {
				assert_eq!(colors.len(), vertices.len());
				Some(gl_vertex_buffer_vec3(shader.color_attrib, colors))
			};

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

        let normal_vbo = shader
            .normal_attrib
            .map(|attr| gl_vertex_buffer_vec3(attr, normals));

        let index_vbo = gl_face_buffer(faces);

        Self {
            object: Object::new(shader),
            pos_vbo,
            normal_vbo,
            color_vbo,
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

/*

class Mesh : public Object {
public:
    // TODO: Also accept an array of normals

    Mesh(const std::vector<glm::vec3> &vertices, const std::vector<glm::vec3> &colors, std::vector<std::vector<unsigned int> > faces, Shader *shader);

    // For colorless meshes using a material
    Mesh(const std::vector<glm::vec3> &vertices, std::vector<std::vector<unsigned int> > faces, Shader *shader);

    ~Mesh();

    // TODO: Generalize this
    static Mesh *read(const char *filename, Shader *shader);

    std::vector<glm::vec3> vertices;

private:
    static Mesh *read_smf(const char *filename, Shader *shader);
    static Mesh *read_stl(const char *filename, Shader *shader);
};

Mesh::~Mesh() {
    // Destroy VBOs
}



*/
