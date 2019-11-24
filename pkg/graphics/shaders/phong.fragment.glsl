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


///////////////////////////////////////////////////////////////


in vec3 fN;
in vec3 fE;
in vec3 fL[MAXLIGHTS];
out vec4 fragColor;


void main() {

	vec3 color = vec3(0,0,0);

	vec3 N = normalize(fN); // Normal
	vec3 E = normalize(fE); // To eye

	for(int i = 0; i < MAXLIGHTS; i++){

		vec3 L = normalize(fL[i]);
		vec3 H = normalize(L + E); // Vertex to eye


		vec3 ambient = material.ambient * lights[i].ambient;

		float Kd = max(dot(L, N), 0.0);
		vec3 diffuse = Kd*(material.diffuse * lights[i].diffuse);

		float Ks = pow(max(dot(N, H), 0.0), material.shininess);
		vec3 specular = Ks*(material.specular * lights[i].specular);
		// discard the specular highlight if the light's behind the vertex
		if(dot(L, N) < 0.0)
			specular = vec3(0.0, 0.0, 0.0);

		color += ambient + diffuse + specular;
	}

	fragColor = vec4(color, 1.0);
}

)"
