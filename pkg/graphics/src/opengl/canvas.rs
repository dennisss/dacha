use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::array::Array;
use math::geometry::half_edge::*;
use math::matrix::{vec2f, Matrix3f, Matrix4f, Vector2f, Vector3f};
use typenum::{U1, U2, U3};

use crate::canvas::base::CanvasBase;
use crate::canvas::*;
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

Caching paths:
- The transform should always go to world coordinates.
    - Ideally decouple into a scaling followed by a translation rotation

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
    fn create_path_object(
        &mut self,
        vertices: &[Vector2f],
        path_starts: &[usize],
    ) -> Result<Box<dyn CanvasObject>> {
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
            if start_i + 3 < end_i {
                for v in &vertices[(start_i + 3)..end_i] {
                    next_edge = half_edges.add_next_edge(next_edge, v.clone());
                }
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
                    new_vertices.push((vert.clone(), 1.).into());
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
        let mut mesh = Mesh::from(
            self.context.clone(),
            &new_vertices,
            &faces,
            &[],
            self.shader.clone(),
        );

        mesh.set_vertex_texture_coordinates(vec2f(0., 0.))
            .set_texture(self.empty_texture.clone());
        Ok(Box::new(OpenGLCanvasPath {
            mesh,
            transform_inv: self.base.current_transform().inverse(),
        }))
    }
}

impl Canvas for OpenGLCanvas {
    // TODO: Implement a clear_rect which uses glClear if we want to remove the
    // entire screen.

    fn create_path_fill(&mut self, path: &Path) -> Result<Box<dyn CanvasObject>> {
        let (vertices, path_starts) = path.linearize(self.current_transform());
        self.create_path_object(&vertices, &path_starts)
    }

    fn create_path_stroke(&mut self, path: &Path, width: f32) -> Result<Box<dyn CanvasObject>> {
        let (vertices, path_starts) = path.stroke(width, self.current_transform());
        self.create_path_object(&vertices, &path_starts)
    }

    fn create_image(&mut self, image: &Image<u8>) -> Result<Box<dyn CanvasObject>> {
        let texture = Rc::new(Texture::new(self.context.clone(), image));
        Ok(Box::new(OpenGLCanvasImage {
            texture,
            width: image.width(),
            height: image.height(),
        }))
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

struct OpenGLCanvasPath {
    mesh: Mesh,
    transform_inv: Matrix3f,
}

impl CanvasObject for OpenGLCanvasPath {
    fn draw(&mut self, paint: &Paint, canvas: &mut dyn Canvas) -> Result<()> {
        // TODO: Verify that the same canvas was passed in.
        let canvas = canvas.as_mut_any().downcast_mut::<OpenGLCanvas>().unwrap();

        // TODO: Have a custom variation of block() with just one dimension type for
        // vectors.
        self.mesh
            .set_vertex_colors(
                paint.color.block::<U3, U1>(0, 0).to_owned().cast::<f32>() / (u8::MAX as f32),
            )
            .set_vertex_alphas(paint.alpha);

        self.mesh.draw(
            &canvas.camera,
            &Transform::from_3f(canvas.base.current_transform() * &self.transform_inv),
        );
        Ok(())
    }
}

struct OpenGLCanvasImage {
    width: usize,
    height: usize,
    texture: Rc<Texture>,
}

impl CanvasObject for OpenGLCanvasImage {
    fn draw(&mut self, paint: &Paint, canvas: &mut dyn Canvas) -> Result<()> {
        let canvas = canvas.as_mut_any().downcast_mut::<OpenGLCanvas>().unwrap();

        let mut rect = Polygon::rectangle(
            canvas.context.clone(),
            vec2f(0.0, 0.0),
            self.width as f32,
            self.height as f32,
            canvas.shader.clone(),
        );

        rect.set_vertex_colors(Vector3f::from_slice(&[1., 1., 1.]))
            .set_texture(self.texture.clone())
            .set_vertex_alphas(paint.alpha);

        rect.draw(&canvas.camera, &Transform::default());
        Ok(())
    }
}
