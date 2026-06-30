use std::rc::Rc;
use std::time::{Instant, Duration};

use common::errors::*;
use file::LocalPathBuf;
use math::matrix::Vector2i;
use graphics::opengl::shader::*;
use math::matrix::{Matrix4f, Vector3f, Vector2f, vec2f, vec2d, vec3f, vec4f};
use graphics::sphere::*;
use graphics::opengl::framebuffer::*;
use image::*;
use graphics::opengl::texture::*;
use math::array::Array;
use vision::*;
use graphics::opengl::drawable::Drawable;
use graphics::opengl::read_buffer::*;
use graphics::transform::Transform;
use graphics::transform::Camera;
use graphics::opengl::app::Application;
use graphics::opengl::window::Window;
use graphics::opengl::polygon::Polygon;
use graphics::opengl::group::Group;
use graphics::opengl::window::WindowContext;
use graphics::opengl::mesh::Mesh;
use graphics::cube::*;
use math::matrix::cwise_binary_ops::*;

use crate::shaders::*;

// The number of render buffers we will allocate.
// - OpenGL will render these sequentially.
// - Once one of these is done rendering, we will read it out to the CPU
//   in parallel to the other ones rendering. 
const NUM_PROJECTED_BUFFERS: usize = 3;


#[derive(Clone)]
pub struct MocapFrameRendererOptions {
    pub cameras: Vec<MocapCameraFrameRendererOptions>,
    pub supersampling: usize,
}

#[derive(Clone)]
pub struct MocapCameraFrameRendererOptions {
    pub camera_id: u64,
    
    pub frame_width: usize,
    pub frame_height: usize,
    pub intrinsics: CameraIntrinsicsModel,
    pub extrinsics: CameraExtrinsics,

    pub z_near: f32,
    pub z_far: f32,
}

pub struct MocapCameraRendererScene {
    pub spheres: Vec<Sphere>,
    pub checkerboard: bool,
    pub cube: bool,
}

#[derive(Clone)]
pub struct Sphere {
    pub center: Vector3f,
    pub radius: f32,
}

pub struct MocapFrameRenderer {
    options: MocapFrameRendererOptions,

    cameras: Vec<PerCameraData>,

    sphere: Mesh,

    cube: Mesh, 

    checkerboard: Group,

    // NOTE: We currently use one buffer for this across all cameras
    // since the memory usage is very high.
    projected_buffers: Vec<FrameBuffer>,

    projected_size: Vector2f,

    window: Window,

    app: Application,
}

struct PerCameraData {
    projected_camera: Camera,    
    distorted_buffer: FrameBuffer,
    distorted_rect: Polygon,
    read_buffer: PixelReadBuffer,
}

impl MocapFrameRenderer {
    pub fn create(options: MocapFrameRendererOptions) -> Result<Self> {
        let mut app = graphics::opengl::app::Application::new();
        let mut window = app.create_window(
            "Compute", Vector2i::from_slice(&[
            1isize, 1isize
            ]),
            false, false,
            true // depth
        );


        let sphere_shader = Rc::new(Shader::load(
            VERTEX_SHADER, SPHERE_FRAGMENT_SHADER, &mut window)?
        );

        let matte_shader = Rc::new(Shader::load(
            MATTE_VERTEX_SHADER, MATTE_FRAGMENT_SHADER, &mut window)?
        );

        let mut sphere = generate_sphere(
            window.context(),
            vec3f(0.0, 0.0, 0.0),
            1.0,
            100,
            100,
            sphere_shader.clone()
        );

        sphere.set_vertex_texture_coordinates(vec2f(0., 0.))
            .set_vertex_colors(vec3f(
                1.0, 1.0, 1.0,
            ));

        let mut cube = generate_cube(
            window.context(),
            vec3f(0.0, 0.0, 0.0),
            1.2,
            // sphere_shader.clone(),
            matte_shader.clone()
        );
        cube.set_vertex_texture_coordinates(vec2f(0., 0.))
            .set_vertex_colors(vec3f(
                0.2, 0.2, 0.2,
            ));


        let mut checkerboard = Self::create_checkerboard(window.context(), sphere_shader.clone());

        let mut t = Matrix4f::identity();
        t[(1, 3)] = -0.3;
        t[(2, 3)] = 0.8;
        checkerboard.set_transform(t);

        // Finding a buffer size that is big enough for all the cameras.
        let mut projected_size = vec2f(0.0, 0.0);
        for cam in &options.cameras {
            let (min, max) = Self::find_undistorted_bounds(cam);
            let size = max - min;
            projected_size = projected_size.cwise_max(size);
        }

        let mut projected_buffers = vec![];
        for _ in 0..NUM_PROJECTED_BUFFERS {
            projected_buffers.push(FrameBuffer::new_with_options(
                window.context(),
                (projected_size.x() as usize) * options.supersampling,
                (projected_size.y() as usize) * options.supersampling,
                {
                    let mut opts = FrameBufferOptions::default();
                    opts.rgb = false;
                    opts
                }
            )?);
        }

        let mut inst = Self {
            options,
            cameras: vec![],
            sphere,
            cube,
            checkerboard,
            projected_buffers,
            projected_size,
            window,
            app
        };

        for i in 0..inst.options.cameras.len() {
            inst.add_camera(i)?;
        }

        Ok(inst)
    }

    fn add_camera(&mut self, camera_idx: usize) -> Result<()> {

        let camera_options = self.options.cameras[camera_idx].clone();

        let (projected_min, projected_max) = Self::find_undistorted_bounds(&camera_options);

        // let projected_size = &projected_max - &projected_min;
        let projected_center = &camera_options.intrinsics.center.clone().cast() - &projected_min;

        // println!("Size: {:?}", projected_size);

        let downsample_shader = Rc::new(Shader::load(VERTEX_SHADER, DISTORTION_FRAGMENT_SHADER, &mut self.window)?);
        downsample_shader
        .set_uniform_int("u_supersampling", self.options.supersampling as i32)?
        .set_uniform_vec2f("u_focal_length", &camera_options.intrinsics.focal_length.clone().cast())?
        .set_uniform_vec2f("u_input_center", &projected_center)?
        .set_uniform_vec2f("u_output_center", &camera_options.intrinsics.center.clone().cast())?
        .set_uniform_float("u_k1", camera_options.intrinsics.k1 as f32)?
        .set_uniform_float("u_k2", camera_options.intrinsics.k2 as f32)?
        .set_uniform_vec2f("u_output_size", &vec2f(camera_options.frame_width as f32, camera_options.frame_height as f32))?
        .set_uniform_vec2f("u_input_size", &self.projected_size)?;

        let mut projected_camera = Camera::default();

        projected_camera.proj =  Self::get_camera_projection_matrix(
            &camera_options.intrinsics.focal_length.clone().cast(),
            &projected_center,
            &self.projected_size,
            camera_options.z_near,
            camera_options.z_far
        );

        projected_camera.view = camera_options.extrinsics.to_mat4x4().cast();

        // TODO: This doesn't need much memory for depth and non-red colors.
        let mut distorted_buffer = FrameBuffer::new_with_options(
            self.window.context(),
            camera_options.frame_width,
            camera_options.frame_height,
            {
                let mut opts = FrameBufferOptions::default();
                opts.rgb = false;
                opts.depth = false;
                opts
            }
        )?;

        let mut distorted_rect = Polygon::rectangle(
            self.window.context(),
            vec2f(-1., -1.),
            2.,
            2.,
            downsample_shader.clone(),
        );
        distorted_rect
        .set_vertex_colors(Vector3f::from_slice(&[1., 1., 1.]));

        let read_buffer = PixelReadBuffer::new(
            self.window.context(),
            camera_options.frame_width * camera_options.frame_height
        )?;

        self.cameras.push(PerCameraData {
            projected_camera,
            distorted_buffer,
            distorted_rect,
            read_buffer,
        });

        Ok(())
    }

    pub fn render(&mut self, scene: &MocapCameraRendererScene) -> Result<Vec<(u64, image::Image<u8>)>> {
        let mut all_frames = vec![];

        // Camera indexes that haven't started being rendered yet. 
        let mut pending_queue = vec![];
        for camera_i in 0..self.cameras.len() {
            pending_queue.push(camera_i);
        }

        // Indexes of projected_buffer entries that aren't currently in use
        let mut free_buffers = vec![];
        for i in 0..self.projected_buffers.len() {
            free_buffers.push(i);
        }

        // List of (camera_i, projected_buffer_i) that are being rendered by the GPU.
        let mut rendering_queue = vec![];

        while pending_queue.len() > 0 || rendering_queue.len() > 0 {
            while pending_queue.len() > 0 && free_buffers.len() > 0 {
                let camera_i = pending_queue.remove(0);
                let buffer_i = free_buffers.remove(0);

                self.render_camera(camera_i, buffer_i, scene);

                rendering_queue.push((camera_i, buffer_i));
            }

            let (camera_i, buffer_i) = rendering_queue.remove(0);
            let image = self.read_camera_pixels(camera_i);
            all_frames.push((self.options.cameras[camera_i].camera_id, image));
            free_buffers.push(buffer_i);
        }

        Ok(all_frames)
    }

    fn render_camera(
        &mut self,
        camera_i: usize,
        projected_buffer_i: usize,
        scene: &MocapCameraRendererScene
    ) {
        let camera = &mut self.cameras[camera_i];
        let camera_options = &self.options.cameras[camera_i];

        {
            let base_model_view = Transform::from(camera.projected_camera.view.clone());

            // TODO: Ideally set the viewport so we only use as much of the buffer as we need for this camera.
            self.projected_buffers[projected_buffer_i].begin_draw();

            unsafe {
                graphics::gl::ClearColor(0.0, 0.0, 0.0, 1.0);
                graphics::gl::Clear(graphics::gl::COLOR_BUFFER_BIT | graphics::gl::DEPTH_BUFFER_BIT);
            }
            
            for v in &scene.spheres {
                let mut m = Matrix4f::identity();
                m[(0, 3)] = v.center[0];
                m[(1, 3)] = v.center[1];
                m[(2, 3)] = v.center[2];

                m[(0, 0)] = v.radius;
                m[(1, 1)] = v.radius;
                m[(2, 2)] = v.radius;

                self.sphere.draw(&camera.projected_camera, &base_model_view.apply(&m));
            } 
            
            if scene.checkerboard {
                self.checkerboard.draw(&camera.projected_camera, &base_model_view);
            }
            
            if scene.cube {
                self.cube.draw(&camera.projected_camera, &base_model_view);
            }


            self.projected_buffers[projected_buffer_i].end_draw();
        }
        
        {
            camera.distorted_buffer.begin_draw();

            // Not needed since we don't use a depth buffer and 
            // unsafe {
            //     graphics::gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            //     graphics::gl::Clear(graphics::gl::COLOR_BUFFER_BIT | graphics::gl::DEPTH_BUFFER_BIT);
            // }

            camera.distorted_rect.set_texture(self.projected_buffers[projected_buffer_i].texture());

            camera.distorted_rect.draw(&Camera::default(), &Transform::default());


            unsafe {
                graphics::gl::PixelStorei(graphics::gl::PACK_ALIGNMENT, 1);
            }

            camera.read_buffer.bind();
            unsafe {
                graphics::gl::ReadPixels(
                    0, 0, camera_options.frame_width as i32, camera_options.frame_height as i32,
                    graphics::gl::RED,
                    graphics::gl::UNSIGNED_BYTE,
                    core::ptr::null_mut()
                );
            }
            camera.read_buffer.unbind();


            camera.distorted_buffer.end_draw();
        }
    }

    fn read_camera_pixels(&mut self, camera_i: usize) -> image::Image<u8> {

        let camera = &mut self.cameras[camera_i];
        let camera_options = &self.options.cameras[camera_i];

        let out = camera.read_buffer.read();

        // Invert y since OpenGL is flipped
        let mut pixels = vec![];
        pixels.reserve_exact(out.len());

        for i in (0..camera_options.frame_height).rev() {
            let j = i * camera_options.frame_width;
            pixels.extend_from_slice(&out[j..(j + camera_options.frame_width)]);
        }

        let image = image::Image {
            array: math::array::Array {
                shape: vec![camera_options.frame_height, camera_options.frame_width, 1],
                data: pixels,
            },
            colorspace: image::Colorspace::Grayscale,
        };

        image
    }


    fn find_undistorted_bounds(options: &MocapCameraFrameRendererOptions) -> (Vector2f, Vector2f) {
        let w = options.frame_width as f64;
        let h = options.frame_height as f64;

        // Initialized to extreme values that should get immediately overriden by the first
        // point tested.
        let mut max = vec2d(0., 0.);
        let mut min = vec2d(w, h);

        let num_samples = 10;

        for i in 0..(num_samples + 1) {
            let v = (i as f64) / (num_samples as f64);

            let pts = [
                vec2d(0., h * v),
                vec2d(w, h * v),
                vec2d(v, 0.),
                vec2d(v, h),
            ];

            for pt in pts {
                let mut p = options.intrinsics.unproject_point(&pt);

                p.cwise_mul_assign(&options.intrinsics.focal_length);
                p += &options.intrinsics.center;

                min = min.cwise_min(&p);
                max = max.cwise_max(&p);
            }
        }


        (
            vec2d(min[0].floor(), min[1].floor()).cast(),
            vec2d(max[0].ceil(), max[1].ceil()).cast()
        )
    }


    // Comes up with an OpenGL projection matrix that correctly maps 3d points from an
    // OpenCV style pinhole camera model / coordinate system onto the OpenGL screen. 
    fn get_camera_projection_matrix(
        focal_length: &Vector2f,
        center: &Vector2f,
        frame_size: &Vector2f,
        z_near: f32,
        z_far: f32
    ) -> Matrix4f {

        // Map from 3d coordinates to pixels
        // This is the standard 'OpenCV'/pinhole camera matrix.
        let proj = {
            let mut out = Matrix4f::identity();
            out[(0, 0)] = focal_length[0];
            out[(1, 1)] = focal_length[1];
            out[(0, 2)] = center[0];
            out[(1, 2)] = center[1];
            out
        };

        // Map from camera space to OpenGL space
        /*
        Camera coordinate system:
        - +x is right (0 - pixel width)
        - -y is up (0 - pixel height)
        - +z is into the screen

        OpenGL coordinate system:
        - +x is right (range: -1 to 1)
        - +y is up
        - -z is into the screen
        */
        let remap = {
            let mut out = Matrix4f::identity();

            // Scaling/shifting x to [-1, 1] range
            out[(0, 0)] = (2.0 / frame_size[0]);
            out[(0, 2)] = -1.0;

            // Scaling/shifting y to [-1, 1] range (and flip it)
            out[(1, 1)] = -(2.0 / frame_size[1]);
            out[(1, 2)] = 1.0;
            
            // Flip z and apply the near and far plane
            out[(2, 2)] = (z_far + z_near) / (z_far - z_near);
            out[(2, 3)] = -(2.0 * z_far * z_near) / (z_far - z_near);
            
            // Copy z into w (OpenGL will divide everything by w after the vertex shader).
            out[(3, 2)] = 1.0;
            out[(3, 3)] = 0.0;

            out
        };

        remap * proj
    }

    fn create_checkerboard(window_context: WindowContext, shader: Rc<Shader>) -> Group {

        let mut out = Group::default();

        let corner_rows = 13;
        let corner_cols = 8;

        let square_size = 0.04; // 40mm

        // 1 square row/column is added on each side as a white border.
        for i in 0..(corner_rows + 3) {
            for j in 0..(corner_cols + 3) {
                let mut rect = Polygon::rectangle(
                    window_context.clone(),
                    vec2f(
                        ((j as f32) - 2.0) * square_size,
                        ((i as f32) - 2.0) * square_size,
                    ),
                    square_size,
                    square_size,
                    shader.clone()
                );                   

                let color = {
                    if i == 0 || i == corner_rows + 2 || j == 0 || j == corner_cols + 2 {
                        // White border
                        Vector3f::from_slice(&[1., 1., 1.])
                    } else if ((i % 2) + j) % 2 == 0 {
                        Vector3f::from_slice(&[1., 1., 1.])
                    } else {
                        Vector3f::from_slice(&[0., 0., 0.])
                    }
                };

                rect.set_vertex_colors(color);

                out.add_object(Box::new(rect));
            }
        }

        out
    }

}
