#include <tansa/graphics/shapes.h>

using namespace std;

namespace tansa{
namespace graphics {

using namespace glm;

float DEGS_TO_RAD = 3.14159f/180.0f;

// See http://andrewnoske.com/wiki/Generating_a_sphere_as_a_3D_mesh
Mesh *SphereHelper(vec3 center, GLfloat radius, unsigned nlat, unsigned nlong,  Shader *shader) {


	vector<vec3> verts;
	vector<vector<unsigned>> faces;

	unsigned p, s, i, j;
	float x, y, z, out;
	unsigned npitch = nlong + 1;

	float pitchInc = (180. / (float)npitch) * DEGS_TO_RAD;
	float rotInc = (360. / (float)nlat) * DEGS_TO_RAD;

	//## PRINT VERTICES:

	// Top and bottom vertices
	verts.push_back({ center.x, center.y + radius, center.z });
	verts.push_back({ center.x, center.y - radius, center.z });

	unsigned fVert = verts.size();    // Record the first vertex index for intermediate vertices.
	for(p = 1; p < npitch; p++) {     // Generate all "intermediate vertices":

		out = radius * sin((float)p * pitchInc);
		if(out < 0) out = -out;
		y = radius * cos(p * pitchInc);
		for(s = 0; s < nlat; s++) {
			x = out * cos(s * rotInc);
			z = out * sin(s * rotInc);

			verts.push_back({ center.x + x, center.y + y, center.z + z });
		}
	}

	//## PRINT SQUARE FACES BETWEEN INTERMEDIATE POINTS:

	for(p = 1; p < npitch - 1; p++) {
		for(s = 0; s < nlat; s++) {
			i = p*nlat + s;
			j = (s == nlat-1) ? i - nlat : i;


			unsigned a = (i-nlat)+fVert,
					 b = (j+1-nlat)+fVert,
					 c = (j+1)+fVert,
					 d = (i)+fVert;


			faces.push_back({a, b, c});
			faces.push_back({a, c, d});

		}
	}

	//## PRINT TRIANGLE FACES CONNECTING TO TOP AND BOTTOM VERTEX:

	// Triangles attached to top and bottom vertices
	unsigned offLastVerts  = fVert + (nlat * (nlong-1));
	for(s = 0; s < nlat; s++) {
		j = (s==nlat-1) ? -1 : s;

		faces.push_back({ fVert-2, (j+1)+fVert, (s)+fVert });
		faces.push_back({ fVert-1, (s)+offLastVerts, (j+1)+offLastVerts });
	}



	return new Mesh(verts, faces, shader);

}




}
}
