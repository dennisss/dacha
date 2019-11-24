use gl::types::{GLuint, GLint};
use crate::drawable::{Object, Drawable};
use math::matrix::Vector3f;
use std::sync::Arc;
use crate::shader::Shader;
use crate::transform::{Camera, Transform};
use std::ptr::null;
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
	pub fn from(vertices: &[Vector3f], colors: &[Vector3f],
				faces: &[Face], mut normals: &[Vector3f],
				shader: Arc<Shader>) -> Self {
		// Setup shader
		// TODO: Do I need to reset the program in the draw?
		unsafe { gl::UseProgram(shader.program); }
		// TODO: Setup uniforms


		let pos_vbo = gl_vertex_buffer_vec3(shader.pos_attrib, vertices);

		let color_vbo =
			if colors.len() == 0 { 0 }
			else {
				assert_eq!(colors.len(), vertices.len());
				gl_vertex_buffer_vec3(shader.color_attrib, colors);
			};

		let mut normal_buffer = vec![];
		if normals.len() == 0 {
			normal_buffer.resize(vertices.len(), Vector3f::zero());
			for face in faces {

			}

		}

		assert_eq!(faces.len(), vertices.len());
		let normal_vbo = gl_vertex_buffer_vec3(shader.normal_attrib, normals);

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


	void draw(Camera *cam, Transform *modelview);


	std::vector<glm::vec3> vertices;

private:

	static Mesh *read_smf(const char *filename, Shader *shader);
	static Mesh *read_stl(const char *filename, Shader *shader);
};


Mesh::Mesh(const vector<vec3> &vertices, vector<vector<unsigned int>> faces, Shader *shader) : Object(shader) {


	// Compute general normals
	vector<vec3> normals(vertices.size(), vec3(0,0,0));
	for(vector<unsigned int> &face : faces){
		// TODO: Check this
		vec3 p0 = vertices[face[0]], p1 = vertices[face[1]], p2 = vertices[face[2]];
		vec3 n = cross(p1 - p0, p2 - p0);

//		if(face[0] >= normals.size() || face[1] >= normals.size() || face[2] >= normals.size())
//			cout << "ERR!" << endl;

		normals[face[0]] += n; normals[face[1]] += n; normals[face[2]] += n;
	}
	for(int i = 0; i < normals.size(); i++){
		normals[i] = normalize(normals[i]);
	}

	vector<GLuint> indices(3*faces.size());
	for(int i = 0; i < faces.size(); i++){
		if(faces[i].size() != 3)
			cerr << "Only triangle meshes are supported" << endl;

		indices[3*i] = faces[i][0];
		indices[3*i + 1] = faces[i][1];
		indices[3*i + 2] = faces[i][2];
	}

	// Create indice buffer
	this->nindices = indices.size();
	glGenBuffers(1, &index_vbo);
	glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, index_vbo);
	glBufferData(GL_ELEMENT_ARRAY_BUFFER, sizeof(GLuint) * indices.size(), &indices[0], GL_STATIC_DRAW);



}


Mesh::~Mesh() {
	// Destroy VBOs
}


Mesh *Mesh::read(const char *filename, Shader *shader){


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
}



*/