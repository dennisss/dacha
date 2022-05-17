#version 330

uniform mat4 proj;
uniform mat4 modelview;

in vec3 position;
in vec2 tex_coord;

out vec2 _tex_coord;

void main(){
    _tex_coord = tex_coord;
	gl_Position = proj * modelview * vec4(position, 1.0);
}