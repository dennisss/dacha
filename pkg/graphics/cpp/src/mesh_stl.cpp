#include <tansa/graphics/mesh.h>
#include "utils.h"

#include <stdio.h>
#include <iostream>

using namespace std;

namespace tansa {
namespace graphics {

// TODO: What about the color?
/// // TODO: Change the return type to bool to indicate success; pass a reference to a vector to use as a parameter
Mesh *Mesh::read_stl(const char *filename, Shader *shader){


	FILE *file = fopen(filename, "rb");
	if(file == NULL)
		return NULL; // Failed to open



	char header[80];
	fread(header, 1, 80, file);


	uint32_t ntriangles;
	fread(&ntriangles, sizeof(uint32_t), 1, file);



	vector<glm::vec3> verts;
	vector<vector<unsigned int>> faces;

	for(unsigned i = 0; i < ntriangles; i++) {

		GLfloat buf[4*3];
		fread(buf, sizeof(GLfloat), 12, file);

		faces.push_back({ (unsigned) verts.size(), (unsigned) verts.size() + 1, (unsigned) verts.size() + 2 });
		verts.push_back(glm::vec3(&buf[3]));
		verts.push_back(glm::vec3(&buf[6]));
		verts.push_back(glm::vec3(&buf[9]));


		uint16_t attrib_size;
		fread(&attrib_size, sizeof(uint16_t), 1, file);
		//fseek(file, attrib_size, SEEK_CUR);
	}

	fclose(file);

	return new Mesh(verts, faces, shader);

}

}
}
