use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::array::Array;
use math::geometry::half_edge::*;
use math::matrix::{vec2f, Matrix4f, Vector2f, Vector3f};
use typenum::{U1, U2, U3};

use crate::canvas::base::CanvasBase;
use crate::canvas::{Canvas, Path};
use crate::opengl::mesh::Mesh;
use crate::opengl::polygon::Polygon;
use crate::opengl::shader::Shader;
use crate::opengl::texture::Texture;
use crate::raster::scanline::ScanLineIterator;
use crate::raster::FillRule;
use crate::transform::{Camera, Transform};

use super::drawable::Drawable;
use super::shader::ShaderAttributeId;
use super::window::WindowContext;

/*
More efficient winding calculation:
- While computing intersections,
    - Store the sum of all winding numbers in each interior tree node.
    - Using this it should be easy to compute the winding in O(log n) time:
        - If our value is the 'right' child of an interior node 'n', compute 'n.value - n.right'
        - If we go 'left', then the value is incremented by 0.
        - Accumulate this value recursively as we go down.
        - The main thing we need to handle to maintain this relationship is

*/

///
/// Some implementation notes:
/// - We always pass an identity model-view matrix to the shader as we always
///   perform transforms on the CPU for the purpose of linearizing paths.
pub struct OpenGLCanvas {
    pub(super) base: CanvasBase,

    pub(super) camera: Camera,

    pub(super) shader: Rc<Shader>,

    /// Reference to a 1x1 texture containing a white pixel. Used as the default
    /// texture if none other is available.
    pub(super) empty_texture: Rc<Texture>,

    pub(super) context: WindowContext,
    // tODO
    // Store a reference to the window in which we are drawing.
}

impl_deref!(OpenGLCanvas::base as CanvasBase);

impl OpenGLCanvas {
    fn fill_path_inner(
        &mut self,
        vertices: &[Vector2f],
        path_starts: &[usize],
        color: &Color,
    ) -> Result<()> {
        let mut half_edges = HalfEdgeStruct::<()>::new();
        for i in 0..(path_starts.len() - 1) {
            let start_i = path_starts[i];
            let end_i = path_starts[i + 1];

            // TODO: Verify has at least 3 vertices.
            let first_edge = half_edges.add_first_edge(
                vertices[start_i].clone(),
                vertices[start_i + 1].clone(),
                (),
            );
            let mut next_edge = half_edges.add_next_edge(first_edge, vertices[start_i + 2].clone());
            for v in &vertices[(start_i + 3)..end_i] {
                next_edge = half_edges.add_next_edge(next_edge, v.clone());
            }
            half_edges.add_close_edge(next_edge, first_edge);
        }

        half_edges.repair();
        half_edges.make_y_monotone();
        half_edges.repair();
        half_edges.triangulate_monotone();
        half_edges.repair();

        let mut new_vertices: Vec<Vector3f> = vec![];
        let mut faces = vec![];

        let all_faces = FaceDebug::get_all(&half_edges);

        let mut face_centroids = vec![];
        for (i, face) in all_faces.iter().enumerate() {
            // Skip the unbounded face.
            if face.outer_component.is_none() {
                continue;
            }

            let mut c = Vector2f::zero();
            for vert in face.outer_component.as_ref().unwrap() {
                c += vert;
            }

            c /= 3.; // Mean of all 3 vertices.

            face_centroids.push((c, i));
        }

        face_centroids.sort_by(|a, b| a.0.y().partial_cmp(&b.0.y()).unwrap());

        let mut iter = ScanLineIterator::create(
            vertices,
            path_starts,
            FillRule::NonZero,
            face_centroids.iter().map(|(c, _)| c.y()),
        )?;

        for (centroid, face_i) in &face_centroids {
            let (_, xs) = iter.next().unwrap();

            let mut winding = 0;
            let mut x_i = 0;
            while x_i < xs.len() && xs[x_i].x < centroid.x() {
                winding += xs[x_i].increment;
                x_i += 1;
            }

            if winding != 0 {
                let face = &all_faces[*face_i];

                faces.push([
                    new_vertices.len() as u32,
                    new_vertices.len() as u32 + 1,
                    new_vertices.len() as u32 + 2,
                ]);

                for vert in face.outer_component.as_ref().unwrap() {
                    new_vertices.push((vert.clone(), 0.).into());
                }
            }
        }

        // TODO: Need to filter these based on winding/even-odd.
        // for face in FaceDebug::get_all(&half_edges) {
        //     if face.outer_component.is_none() {
        //         continue;
        //     }
        // }

        // TODO: Make sure that this doesn't try computing any normals.
        let mut mesh = Mesh::from(&new_vertices, &faces, &[], self.shader.clone());

        // TODO: Have a custom variation of block() with just one dimension type for
        // vectors.
        mesh.set_vertex_colors(
            color.block::<U3, U1>(0, 0).to_owned().cast::<f32>() / (u8::MAX as f32),
        )
        .set_vertex_texture_coordinates(vec2f(0., 0.))
        .set_vertex_alphas(1.)
        .set_texture(self.empty_texture.clone());

        mesh.draw(&self.camera, &Transform::default());
        Ok(())
    }
}

impl Canvas for OpenGLCanvas {
    // TODO: Implement a clear_rect which uses glClear if we want to remove the
    // entire screen.

    fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()> {
        let (vertices, path_starts) = path.linearize(self.base.current_transform());
        self.fill_path_inner(&vertices, &path_starts, color)
    }

    fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()> {
        let (verts, path_starts) = path.linearize(self.base.current_transform());

        let scale = self.base.current_transform()[(0, 0)];
        let width_scaled = width * scale;

        let dash_array = &[]; // &[5.0 * scale, 5.0 * scale];

        for (i, j) in path_starts.pair_iter() {
            let dashes = crate::raster::stroke::stroke_split_dashes(&verts[*i..*j], dash_array);

            for dash in dashes {
                let (points, starts) = crate::raster::stroke::stroke_poly(&dash, width_scaled);

                // TODO: Use non-zero winding
                self.fill_path_inner(&points, &starts, color)?;
            }
        }

        Ok(())
    }

    fn load_image(&mut self, image: &Image<u8>) -> Result<Box<dyn Any>> {
        let texture = Rc::new(Texture::new(self.context.clone(), image));
        Ok(Box::new(OpenGLCanvasImage {
            texture,
            width: image.width(),
            height: image.height(),
        }))
    }

    fn draw_image(&mut self, image: &dyn Any, alpha: f32) -> Result<()> {
        let image = image.downcast_ref::<OpenGLCanvasImage>().unwrap();

        let mut rect = Polygon::rectangle(
            vec2f(0.0, 0.0),
            image.width as f32,
            image.height as f32,
            self.shader.clone(),
        );

        rect.set_vertex_colors(Vector3f::from_slice(&[1., 1., 1.]))
            .set_texture(image.texture.clone())
            .set_vertex_texture_coordinates(&[
                vec2f(0.0, -1.),
                vec2f(1.0, -1.),
                vec2f(1.0, 0.0),
                vec2f(0.0, 0.0),
            ])
            .set_vertex_alphas(alpha);

        rect.draw(&self.camera, &Transform::default());
        Ok(())
    }

    /*
    OpenGL rounded corner rendering
    - Could be implemented as a fragment shader.
    - Mainly need to know the x,y of the circle center.
    - Compute distance to the center
    - Render if pixel is
    */

    /*

    fill_path
    -> Hard.
        For each closed path segment
            Linearize any arc segments.
            Generate HalfEdgeStruct and label each label post repair with winding increment
        Perform overlap of all path structs.
        Do monotone conversion and triangulation.

        The final output is a list of triangles to actually draw!
        => Ideally we would cache this to avoid re-computing every time (although this is scale dependent)

        Some challenges:
        - Handling clipping
        - Can only use a path handle on the same canvas that twas used to create it (need globally unique ids or references).


    - Segment into non-overlapping and non-intersecting paths

        - Linearize
        - To monotone polygons
        - To triangles



    stroke_path
    -> Medium. linearize and then just

    fill_rectangle
    -> Easy. Optimize as two triangles.

    stroke_



    */
}

struct OpenGLCanvasImage {
    width: usize,
    height: usize,
    texture: Rc<Texture>,
}
