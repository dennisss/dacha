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

out vec3 _color;


void main(){

	_color = vec3(0,0,0);


	// TODO: Not used
	// Transform vertex position into eye coordinates
	vec3 pos = (modelview * vec4(position, 1.0)).xyz;

	// Transform vertex normal into eye coordinates
	vec3 N = normalize(normal); //normalize(modelview * vec4(normal, 0.0)).xyz;
	vec3 E = normalize(eyePosition - position.xyz);

	for(int i = 0; i < MAXLIGHTS; i++){

		vec3 L;
		if(lights[i].position.w != 0.0){
			// Light defined in world coordinates
			L = normalize(lights[i].position.xyz - position);
		}
		else {
			L = normalize(eyePosition - lights[i].position.xyz);
		}



		vec3 H = normalize( L + E ); // Vector from vertex to eye (used for specular)

		// Compute terms in the illumination equation
		vec3 ambient = lights[i].ambient * material.ambient;

		vec3 diffuseProduct = lights[i].diffuse * material.diffuse;
		float Kd = max(dot(L, N), 0.0);
		vec3 diffuse = Kd*diffuseProduct;

		vec3 specularProduct = lights[i].specular * material.specular;
		float Ks = pow( max(dot(N, H), 0.0), material.shininess );
		vec3 specular = Ks * specularProduct;
		if(dot(L, N) < 0.0) specular = vec3(0.0, 0.0, 0.0);

		_color += ambient + diffuse + specular;

	}

	gl_Position = proj * modelview * vec4(position, 1.0);
}

)"
