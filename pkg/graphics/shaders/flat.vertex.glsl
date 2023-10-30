#version 330

// TODO: Stop calling this 'flat' shading as it doesn't use any lights.

uniform mat4 proj;
uniform mat4 modelview;

in vec3 position;
in vec3 color;

out vec3 _color;

void main(){
	_color = color;
	gl_Position = proj * modelview * vec4(position, 1.0);
}