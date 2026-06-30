use std::rc::Rc;
use std::f32::consts::PI;

use math::matrix::{vec3f, Vector3f};

use crate::opengl::window::*;
use crate::opengl::mesh::*;
use crate::opengl::shader::*;


const DEGS_TO_RAD: f32 = PI / 180.0;

// See http://andrewnoske.com/wiki/Generating_a_sphere_as_a_3D_mesh
pub fn generate_sphere(
    window_context: WindowContext,
    center: Vector3f,
    radius: f32,
    nlat: usize,
    nlong: usize,
    shader: Rc<Shader>
) -> Mesh {

    let mut verts = vec![];
    let mut faces = vec![];

    let npitch = nlong + 1;
    
	let pitch_inc = (180. / (npitch as f32)) * DEGS_TO_RAD;
	let rot_inc = (360. / (nlat as f32)) * DEGS_TO_RAD;

	// Top and bottom vertices
	verts.push(vec3f(center.x(), center.y() + radius, center.z()));
	verts.push(vec3f(center.x(), center.y() - radius, center.z()));

    // Intermediate verticies
    let f_vert = verts.len();
    for p in 1..npitch {
        let p = p as f32;

		let mut out = radius * (p * pitch_inc).sin();
		if out < 0.0 { out = -out; }

		let y = radius * (p * pitch_inc).cos();
		
        for s in 0..nlat {
            let s = s as f32;
			let x = out * (s * rot_inc).cos();
			let z = out * (s * rot_inc).sin();
			verts.push(vec3f(center.x() + x, center.y() + y, center.z() + z));
        }
    }

    // Generating intermediate faces.
    for p in 1..(npitch - 1) {
        for s in 0..nlat {
            let i = p*nlat + s;
            let j = if s == nlat - 1 { i - nlat } else { i };

			let a = (i-nlat) + f_vert;
            let b = (j+1-nlat) + f_vert;
            let c = (j+1) + f_vert;
            let d = (i) + f_vert;

			faces.push([a, b, c]);
			faces.push([a, c, d]);
        }
    }

	// Triangles attached to top and bottom vertices
	let off_last_verts  = f_vert + (nlat * (nlong-1));
	for s in 0..nlat {
		let j = if s == nlat-1 { 0 } else { s + 1 };

		faces.push([ f_vert - 2, j + f_vert, s + f_vert ]);
		faces.push([ f_vert - 1, s + off_last_verts, j + off_last_verts ]);
	}

    let faces = faces.into_iter().map(|v| [v[0] as u32, v[1] as u32, v[2] as u32]).collect::<Vec<_>>();

    Mesh::from(
        window_context,
        &verts,
        &faces,
        &[], // normals
        shader
    )
}
