/* My implementation of the OpenGL Mathematics Library */

#ifndef TANSA_GRAPHICS_GLM_H_
#define TANSA_GRAPHICS_GLM_H_

#include <GL/glew.h>
#include <GLFW/glfw3.h>

#include <cmath>

namespace tansa {
namespace graphics {
namespace glm {

inline mat4 perspective(GLfloat fovy, GLfloat aspect, GLfloat near,
                        GLfloat far) {
  GLfloat top = tan(fovy / 2) * near;
  GLfloat right = top * aspect;

  GLfloat a = -(far + near) / (far - near);
  GLfloat b = (-2.0 * near * far) / (far - near);

  return mat4(near / right, 0, 0, 0, 0, near / top, 0, 0, 0, 0, a, b, 0, 0, -1,
              0);
}

inline mat4 lookAt(vec3 eye, vec3 center, vec3 up) {
  vec3 n = normalize(eye - center);
  vec3 uu = normalize(cross(up, n));
  vec4 u = vec4(uu.x, uu.y, uu.z, 0.0);
  vec3 vv = normalize(cross(n, uu));
  vec4 v = vec4(vv.x, vv.y, vv.z, 0.0);
  vec4 t = vec4(0.0, 0.0, 0.0, 1.0);

  mat4 c(u.x, u.y, u.z, 0, v.x, v.y, v.z, 0, n.x, n.y, n.z, 0, 0, 0, 0, 1);

  return c * translate(vec3(-eye.x, -eye.y, -eye.z));
}

}  // namespace glm
}  // namespace graphics
}  // namespace tansa

#endif
