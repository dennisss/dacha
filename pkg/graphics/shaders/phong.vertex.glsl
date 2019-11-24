R"(

#version 330

#define MAXLIGHTS 2

struct LightSource {
	vec4 position;
	vec3 ambient;
	vec3 diffuse;
	vec3 specular;
};

struct Material {
	vec3 ambient;
	vec3 diffuse;
	vec3 specular;
	float shininess;
};


uniform int nlights;
uniform LightSource lights[MAXLIGHTS];
uniform Material material;

uniform mat4 proj;
uniform mat4 modelview;
uniform vec3 eyePosition;

in vec3 position;
in vec3 normal;


//////////////////////////////////////////////////////////////


// output values that will be interpolated per-fragment
out vec3 fN; // fragment normal
out vec3 fE; // vector from vertex to camera
out vec3 fL[MAXLIGHTS];


void main(){

	fN = normal;
	fE = eyePosition - position.xyz;

	for(int i = 0; i < MAXLIGHTS; i++){
		if(lights[i].position.w != 0.0){
			// Light defined in world coordinates
			fL[i] = lights[i].position.xyz - position;
		}
		else {
			fL[i] = eyePosition - lights[i].position.xyz;
		}
	}

	gl_Position = proj * modelview * vec4(position, 1.0);
}

)"
