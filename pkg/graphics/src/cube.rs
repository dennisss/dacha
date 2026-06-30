use std::rc::Rc;
use math::matrix::{vec3f, Vector3f};

use crate::opengl::window::*;
use crate::opengl::mesh::*;
use crate::opengl::shader::*;

pub fn generate_cube(
    window_context: WindowContext,
    center: Vector3f,
    size: f32,
    shader: Rc<Shader>
) -> Mesh {
    
    let hs = size / 2.0; // Half-size for offsets
    let cx = center.x();
    let cy = center.y();
    let cz = center.z();

    // 8 Vertices of the cube
    let verts = vec![
        vec3f(cx - hs, cy - hs, cz - hs), // 0: Left, Bottom, Back
        vec3f(cx + hs, cy - hs, cz - hs), // 1: Right, Bottom, Back
        vec3f(cx + hs, cy + hs, cz - hs), // 2: Right, Top, Back
        vec3f(cx - hs, cy + hs, cz - hs), // 3: Left, Top, Back
        vec3f(cx - hs, cy - hs, cz + hs), // 4: Left, Bottom, Front
        vec3f(cx + hs, cy - hs, cz + hs), // 5: Right, Bottom, Front
        vec3f(cx + hs, cy + hs, cz + hs), // 6: Right, Top, Front
        vec3f(cx - hs, cy + hs, cz + hs), // 7: Left, Top, Front
    ];

    // 12 Triangular Faces using standard Counter-Clockwise (CCW) winding order
    let faces: Vec<[u32; 3]> = vec![
        // Front face
        [4, 5, 6], [4, 6, 7],
        // Right face
        [5, 1, 2], [5, 2, 6],
        // Back face
        [1, 0, 3], [1, 3, 2],
        // Left face
        [0, 4, 7], [0, 7, 3],
        // Top face
        [7, 6, 2], [7, 2, 3],
        // Bottom face
        [0, 1, 5], [0, 5, 4],
    ];

    Mesh::from(
        window_context,
        &verts,
        &faces,
        &[], // normals
        shader
    )
}