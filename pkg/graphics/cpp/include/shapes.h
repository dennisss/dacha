#ifndef TANSA_GRAPHICS_SHAPES_H_
#define TANSA_GRAPHICS_SHAPES_H_

#include "drawable.h"
#include "mesh.h"

namespace tansa {
namespace graphics {


Mesh *SphereHelper(glm::vec3 center, GLfloat radius, unsigned nlat, unsigned nlong, Shader *shader);



}
}


#endif
