/* My implementation of the OpenGL Mathematics Library */

#ifndef TANSA_GRAPHICS_GLM_H_
#define TANSA_GRAPHICS_GLM_H_

#include <GL/glew.h>
#include <GLFW/glfw3.h>

#include <cmath>


namespace tansa {
namespace graphics {
namespace glm {


struct vec4 {
	vec4(GLfloat x = 0, GLfloat y = 0, GLfloat z = 0, GLfloat w = 0){ this->x = x; this->y = y; this->z = z; this->w = w; };
	vec4(const vec3 &v, GLfloat w) : vec4(v.x, v.y, v.z, w){};

	GLfloat operator[](int i){
		return value[i];
	};

	union {
		struct {
			GLfloat x;
			GLfloat y;
			GLfloat z;
			GLfloat w;
		};
		GLfloat value[3];
	};


};

struct mat4 {

	mat4(vec4 a, vec4 b, vec4 c, vec4 d){
		value[0] = a;
		value[1] = b;
		value[2] = c;
		value[3] = d;
	}

	mat4(GLfloat v = 1.0f){
		value[0] = vec4(v, 0, 0, 0);
		value[1] = vec4(0, v, 0, 0);
		value[2] = vec4(0, 0, v, 0);
		value[3] = vec4(0, 0, 0, v);
	};

	mat4(GLfloat m00, GLfloat m01, GLfloat m02, GLfloat m03,
		GLfloat m10, GLfloat m11, GLfloat m12, GLfloat m13,
		GLfloat m20, GLfloat m21, GLfloat m22, GLfloat m23,
		GLfloat m30, GLfloat m31, GLfloat m32, GLfloat m33
	){

		value[0] = vec4(m00, m10, m20, m30);
		value[1] = vec4(m01, m11, m21, m31);
		value[2] = vec4(m02, m12, m22, m32);
		value[3] = vec4(m03, m13, m23, m33);
	};


	vec4 value[4];
};


inline vec4 operator*(const mat4 &lhs, const vec4 &rhs){
	return vec4(
		(lhs.value[0].x * rhs.x) + (lhs.value[1].x * rhs.y) + (lhs.value[2].x * rhs.z) + (lhs.value[3].x * rhs.w),
		(lhs.value[0].y * rhs.x) + (lhs.value[1].y * rhs.y) + (lhs.value[2].y * rhs.z) + (lhs.value[3].y * rhs.w),
		(lhs.value[0].z * rhs.x) + (lhs.value[1].z * rhs.y) + (lhs.value[2].z * rhs.z) + (lhs.value[3].z * rhs.w),
		(lhs.value[0].w * rhs.x) + (lhs.value[1].w * rhs.y) + (lhs.value[2].w * rhs.z) + (lhs.value[3].w * rhs.w)
	);
}

inline mat4 operator*(const mat4 &lhs, const mat4 &rhs){
	mat4 m;
	m.value[0] = lhs * rhs.value[0];
	m.value[1] = lhs * rhs.value[1];
	m.value[2] = lhs * rhs.value[2];
	m.value[3] = lhs * rhs.value[3];
	return m;
}


// Transforms



inline mat4 perspective(GLfloat fovy, GLfloat aspect, GLfloat near, GLfloat far){

	GLfloat top = tan(fovy/2) * near;
	GLfloat right = top * aspect;

	GLfloat a = -(far + near) / (far - near);
	GLfloat b = (-2.0*near*far) / (far - near);

	return mat4(
		near/right, 0, 0, 0,
		0, near/top, 0, 0,
		0, 0, a, b,
		0, 0, -1, 0
	);
}



inline mat4 ortho(GLfloat left, GLfloat right, GLfloat bottom, GLfloat top, GLfloat zNear, GLfloat zFar){

	return mat4(
		2.0/(right-left), 0, 0, -(right+left)/(right-left),
		0, 2.0/(top-bottom), 0, -(top+bottom)/(top-bottom),
		0, 0, -2.0/(zFar-zNear), -(zFar+zNear)/(zFar-zNear),
		0, 0, 0, 1
	);

}


inline mat4 lookAt(vec3 eye, vec3 center, vec3 up){

	vec3 n = normalize(eye - center);
	vec3 uu = normalize(cross(up, n));
	vec4 u = vec4(uu.x, uu.y, uu.z, 0.0);
	vec3 vv = normalize(cross(n,uu));
	vec4 v = vec4(vv.x, vv.y, vv.z, 0.0);
	vec4 t = vec4(0.0, 0.0, 0.0, 1.0);

	mat4 c(
		u.x, u.y, u.z, 0,
		v.x, v.y, v.z, 0,
		n.x, n.y, n.z, 0,
		0, 0, 0, 1
	);

	return c * translate(vec3(-eye.x, -eye.y, -eye.z));
}


}
}
}




#endif
