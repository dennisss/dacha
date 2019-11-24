
pub struct Patch {
//	object: Object

}


/*
/* A Bezier Patch : Consists of a set of control points, and holds a reference to an interpolated mesh */
class Patch : public Object {

public:

	static Patch *read(char *filename, Shader *shader);


	// control_points should be of size 16 representing a 4*4 grid
	Patch(std::vector<glm::vec3> &control_points, Shader *shader);
	~Patch();

	void draw(Camera *cam, Transform *modelview);


	void generate(int res);

	// The points backing it
	std::vector<glm::vec3> control_points;

private:

	// The currently generated mesh
	Mesh *mesh;
	Shader *shader;
};



Patch::Patch(vector<vec3> &control_points, Shader *shader) : Object(shader) {
	mesh = NULL;

	this->control_points = control_points;
	this->shader = shader;


}

Patch::~Patch(){
	if(this->mesh != NULL)
		delete this->mesh;
}

// Compute bezier blending coeffients at a given parameter value
vec4 bez(float t){
	float a = pow(1.0-t, 3);
	float b = 3.0*t*pow(1.0-t, 2);
	float c = 3.0*t*t*(1.0 - t);
	float d = t*t*t;

	return vec4(a,b,c,d);
}

void Patch::generate(int res){

	int width = res + 1;
	vector<vec3> pts(width*width);
	vector<vector<unsigned int>> faces;

	float del = 1.0 / res;


	// Generate points
	for(int ui = 0; ui < width; ui++){
		for(int vi = 0; vi < width; vi++){

			float u = ui*del, v = vi*del;

			for(int i = 0; i <= 3; i++){
				for(int j = 0; j <= 3; j++){
					vec4 bu = bez(u);
					vec4 bv = bez(v);
					pts[ui*width + vi] = bu[i]*bv[j]*control_points[4*i + j];
				}
			}
		}
	}


	// Generate faces
	for(int i = 0; i < res; i++){
		for(int j = 0; j <= res; j++){

			unsigned ind00 = i*width + j;
			unsigned ind01 = ind00 + 1;
			unsigned indn11 = ind01 - width;
			unsigned ind10 = ind00 + width;

			/*
				Face topology
				'*' indicates the current point

						-
			          / |
			         / 1|
					* - -
					| 2 /
					|  /
					|-/
			*/


			// Face 1
			if((i-1) >= 0 && (j+1) < width){
				faces.push_back({ind00, ind01, indn11});
			}

			// Face 2
			if((i+1) < width && (j+1) < width){
				faces.push_back({ind00, ind10, ind01});
			}
		}
	}


	this->mesh = new Mesh(pts, faces, shader);

}




Patch *Patch::read(char *filename, Shader *shader){
	ifstream fs(filename);

	vector<vec3> cpts;

	for(int i = 0; i < 16; i++){
		float x, y, z;
		fs >> x; fs >> y; fs >> z;
		cpts.push_back(vec3(x,y,z));
	}


	return new Patch(cpts, shader);
}

void Patch::draw(Camera *cam, Transform *modelview){
	// TODO: Apply my transform 'trans'?
	this->mesh->draw(cam, modelview);
}


*/