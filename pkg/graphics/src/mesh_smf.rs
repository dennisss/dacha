

/*

// TODO: Probably best to distinguish a Mesh (indice based triangles) from a bunch of triangles (Polygon)

// The Polygon class can use a TRIANGLE_FAN, so only needs to specify the outer vertices once

// TODO: What about the color?
/// // TODO: Change the return type to bool to indicate success; pass a reference to a vector to use as a parameter
Mesh *Mesh::read_smf(const char *filename, Shader *shader){

	ifstream file(filename);

	if(!file.is_open())
		return NULL; // Failed to open

	bool started = false;


	vector<glm::vec3> verts;
	vector<vector<unsigned int>> faces;
	//Mesh *mesh = new Mesh();


	string line;
	while(getline(file, line)){

		vector<string> ops;

		char *tok = spacetok((char *) line.c_str());
		while(tok != NULL){
			ops.push_back(tok);
			tok = spacetok(NULL);
		}


		if(ops.size() == 0){ // Empty line
			continue;
		}

		string name = ops[0];

		std::transform(name.begin(), name.end(), name.begin(), ::tolower);

		bool success = true;
		if(name.compare("v") == 0){ // Vertex - parse as doubles
			if(ops.size() != 4){
				cerr << "Invalid vertex" << endl;
			}

			glm::vec3 v;
			success &= parse_float(ops[1].c_str(), &v.x);
			success &= parse_float(ops[2].c_str(), &v.y);
			success &= parse_float(ops[3].c_str(), &v.z);

			verts.push_back(v);

		}
		else if(name.compare("f") == 0){ // Face - parse as integers
			if(ops.size() != 4){
				cerr << "Invalid face" << endl;
			}

			vector<unsigned int> f(3);
			//Face f;
			success &= parse_int(ops[1].c_str(), (int *) &f[0]);
			success &= parse_int(ops[2].c_str(), (int *) &f[1]);
			success &= parse_int(ops[3].c_str(), (int *) &f[2]);

			f[0]--; f[1]--; f[2]--;

			// TODO: Check that all vertices are in bounds

			faces.push_back(f);

		}
		else{
			// Invalid command
			cerr << "Invalid command in smf file" << endl;
		}

		if(!success){
			cerr << "An error occured in parsing the smf file" << endl;
			return NULL;
		}

		// TODO: Check that all faces are in range



	}


	file.close();


	cout << "REad " << verts.size() << " " << faces.size() << endl;

	return new Mesh(verts, faces, shader);
}

*/