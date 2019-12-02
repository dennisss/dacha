use gl::types::{GLuint, GLint};
use math::matrix::Vector3f;
use std::sync::Arc;
use std::ptr::null;
use async_std::path::Path;
use crate::drawable::{Object, Drawable};
use crate::shader::Shader;
use crate::transform::{Camera, Transform};
use crate::util::gl_vertex_buffer_vec3;

pub type Face = [GLuint; 3];

/// Drawing of generic triangle based meshes.
pub struct Mesh {
	object: Object,

	pos_vbo: GLuint,
	normal_vbo: GLuint,
	color_vbo: GLuint,
	index_vbo: GLuint,
	nindices: usize
}

impl_deref!(Mesh::object as Object);

// TODO: Do we need to use glUseProgram before using the
impl Mesh {
	pub async fn read(path: &str, shader: Arc<Shader>) -> Result<Self> {
		/*
		const char *ext = get_filename_ext(filename);


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

	pub fn from(vertices: &[Vector3f], colors: &[Vector3f],
				faces: &[Face], mut normals: &[Vector3f],
				shader: Arc<Shader>) -> Self {
		// Setup shader
		// TODO: Do I need to reset the program in the draw?
		unsafe { gl::UseProgram(shader.program); }
		// TODO: Setup uniforms


		let pos_vbo = gl_vertex_buffer_vec3(shader.pos_attrib, vertices);

		let color_vbo =
			// If no colors are provided, we will not bind any value to the
			// attribute and will instead assume that the material is handling
			// all of the coloring
			if colors.len() == 0 { 0 }
			else {
				assert_eq!(colors.len(), vertices.len());
				gl_vertex_buffer_vec3(shader.color_attrib, colors);
			};

		// TODO: Verify that all faces have in-range indices

		// When no normals are specified, we will generate normals by averaging
		// all faces connected to each vertex.
		let mut normal_buffer = vec![];
		if normals.len() == 0 {
			normal_buffer.resize(vertices.len(), Vector3f::zero());
			for face in faces {
				let p0 = &vertices[face[0]];
				let p1 = &vertices[face[1]];
				let p2 = &vertices[face[2]];
				let n = (p1 - p0).cross(&(p2 - p0));
				
				for idx in face {
					normal_buffer[idx] += n;
				}
			}

			normals = &normal_buffer;
		}

		let normal_vbo = gl_vertex_buffer_vec3(shader.normal_attrib, normals);

		let index_vbo = gl_face_buffer(faces);

		Self {
			object: Object::from(shader),
			pos_vbo, normal_vbo, color_vbo, index_vbo, nindices: indices.len()
		}
	}
}

impl Drawable for Mesh {
	fn draw(&self, camera: &Camera, prev: &Transform) {
		self.object.draw(camera, prev);
		unsafe {
			gl::DrawElements(gl::TRIANGLES, self.nindices, gl::UNSIGNED_INT,
							 null());
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