#version 330

in vec2 _tex_coord;

uniform sampler2D _texture;

out vec4 fragColor;

void main(){
	fragColor = texture(_texture, _tex_coord);
}
