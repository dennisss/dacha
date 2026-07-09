use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::f32::consts::PI;

use common::errors::*;
use common::hash::*;
use common::bytes::Bytes;
use executor_multitask::{impl_resource_passthrough, TaskResource, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use mocap_proto::mocap::*;
use vision::*;
use file::{project_path, LocalPathBuf};
use cluster_client::id::entity_id_from_string;
use executor::sync::{SyncMutex, AsyncMutex};
use executor::{lock, lock_async};
use protobuf_json::*;
use math::matrix::{vec3d, vec3f, Matrix4f, Vector3f, Vector3d};
use mocap_camera_core::FrameProcessor;
use image::format::jpeg::encoder::JPEGEncoder;
use image::Image;
use math::matrix::axis_angle::from_axis_angle;
use crypto::random::*;

use crate::renderer::*;


const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(1000 / 30);

const MIN_JPEG_INTERVAL: Duration = Duration::from_millis(1000 / 5);


pub struct MocapSimulator {
    shared: Arc<Shared>,
    // resource: TaskResource
}

struct Shared {
    cameras: HashMap<u64, Camera, FastHasherBuilder>,
    animation_config: SyncMutex<SimulationAnimationConfig>,
    wand_points: Vec<Vector3d>,
}

#[derive(Default)]
struct Camera {
    blob_subscribers: BroadcastChannel<ReadBlobsResponse>,
    mjpeg_subscribers: BroadcastChannel<Bytes>,
    status: SyncMutex<MocapCameraStatus>
}

impl MocapSimulator {
    // TODO :Have this take as input a config container.
    pub async fn create(manager_config: &MocapManagerConfig) -> Result<Self> {
        let mut renderer_options = MocapFrameRendererOptions {
            supersampling: 4, // 8^2 = 64 samples per pixel.
            cameras: vec![],
        };

        let data = file::read_to_string(project_path!("pkg/vision/mocap/camera/js/dummy_status.json")).await?;
        let mut status = MocapCameraStatus::parse_json(&data, &ParserOptions::default())?;

        status.config_mut().set_frame_rate(0u32);

        let mut cameras = HashMap::default();

        for per_cam in manager_config.per_camera() {
            // TODO: Assume already done.
            let camera_id = per_cam.camera_id();

            renderer_options.cameras.push(MocapCameraFrameRendererOptions {
                camera_id,
                frame_width: 1920,
                frame_height: 1200,
                intrinsics: CameraIntrinsicsModel::from_proto(per_cam.intrinsics()),
                extrinsics: CameraExtrinsics::from_proto(per_cam.extrinsics()),
                z_far: 10.0,
                z_near: 0.1,
            });
            
            cameras.insert(camera_id, Camera {
                blob_subscribers: Default::default(),
                mjpeg_subscribers: Default::default(),
                status: SyncMutex::new(status.clone())
            });
        }


        let c = manager_config.wand();
        let wand_points = vec![
            vec3d(-c.left_arm_length(), 0., 0.),
            vec3d(0.,  0., 0.),
            vec3d(c.right_arm_length(), 0., 0.),
            vec3d(0., -c.bottom_length(), 0.),
        ];
        
        let shared = Arc::new(Shared {
            cameras,
            animation_config: SyncMutex::default(),
            wand_points,
        });

        let shared2 = shared.clone();
        std::thread::spawn(move || {
            Self::render_thread(shared2, renderer_options).unwrap()
        });

        // TODO: OpenGL is blocking and likes to just be on one thread at a time.
        // let resource = TaskResource::spawn_interruptable("MocapSimulator", Self::render_thread(shared.clone(), renderer));

        Ok(Self {
            shared,
            // resource
        })
    }

    pub fn create_camera_service(&self, camera_id: u64) -> Result<Arc<dyn rpc::Service>> {
        if !self.shared.cameras.contains_key(&camera_id) {
            return Err(err_msg("No such camera id"));
        }

        Ok(SimulatedMocapCamera {
            camera_id,
            shared: self.shared.clone(),
        }.into_service())
    }

    pub fn configure_animation(&self, config: &SimulationAnimationConfig) -> Result<()> {
        self.shared.animation_config.apply(|c| {
            *c = config.clone();
        })?;

        Ok(())
    }


    fn render_thread(
        shared: Arc<Shared>,
        renderer_options: MocapFrameRendererOptions,
    ) -> Result<()> {
        let mut renderer = MocapFrameRenderer::create(renderer_options)?;

        // TODO: Don't hardcode the size.
        let mut frame_processor = FrameProcessor::new(1920, 1200);

        let mut inst = RendererThread {
            shared: shared.clone(),
            renderer,
            frame_processor,
            last_rendered: None,
            jpeg_encoder: {
                let mut encoder = JPEGEncoder::new(100);
                encoder.use_default_tables();
                encoder
            }
        };

        // executor::sleep(Duration::from_millis(10)).await?;

        let mut last_jpeg = Instant::now();

        let mut frame_i = 0;
        loop {


            let s = Instant::now();

            let animation_config = shared.animation_config.apply(|mut config| {
                let c = config.clone();

                if config.playing() {
                    config.set_current_frame(c.current_frame() + 1);
                }

                c
            })?;

            let frame_time = Duration::from_secs_f64((frame_i as f64) * (1.0 / 30.0));

            let scene = Self::generate_scene(&shared, &animation_config);

            let mut request = RenderRequest {
                scene,
                camera_settings: Default::default()
            };

            for (camera_id, cam) in &shared.cameras {
                request.camera_settings.insert(*camera_id, cam.status.apply(|status| {
                    CameraSettings {
                        blob_threshold: status.config().pixel_threshold() as u8,
                        blob_filter: status.config().blob_filter().clone(),
                        running: status.config().frame_rate() != 0
                    }
                })?);
            }
                

            let out = inst.render_once(&request)?;

            // Push all cameras at once to ensure they aren't received too out of order by the manager.
            for mut results in out {
                results.set_frame_timestamp(frame_time.as_nanos() as u64);

                let camera_id = results.cameras()[0].camera_id();
                inst.shared.cameras.get(&camera_id).unwrap().blob_subscribers.send(results);
            }

            {
                let now = Instant::now();
                if now - last_jpeg > MIN_JPEG_INTERVAL {

                    for (camera_id, cam) in &shared.cameras {
                        if !cam.mjpeg_subscribers.active() {
                            continue;
                        }

                        let frame = match inst.get_encoded_frame(*camera_id)? {
                            Some(v) => v,
                            None => continue
                        };
                        cam.mjpeg_subscribers.send(frame);
                    }

                    last_jpeg = now;
                }
            }


            let e = Instant::now();

            // if frame_i % 30 == 0 {
            //     println!("Frame Render Time: {:?}", e - s);
            // }


            if e - s < MIN_FRAME_INTERVAL {
                std::thread::sleep(MIN_FRAME_INTERVAL - (e - s));
            }

            frame_i += 1;
        }
    }

    fn generate_scene(shared: &Shared, config: &SimulationAnimationConfig) -> MocapCameraRendererScene {

        let mut scene = MocapCameraRendererScene::default();
        
        if config.marker_around_cube() {
            let angle_i = config.current_frame() % 100;
            let angle = 2.0 * PI * (angle_i as f32) / 100.0;
            let point_center = vec3f(angle.cos(), angle.sin(), 0.0);

            scene.spheres.push(Sphere {
                center: point_center.clone(),
                radius: 0.02
            });
            scene.cube = true;
        }

        if config.checkerboard() {
            let square_size = 0.04 as f32;
            let grid_width = (8 as f32) * square_size;
            let grid_height = (13 as f32) * square_size;

            let z_near = 0.7;
            let z_mid = 1.0;

            // (z, angle, offset)
            let mut variants = vec![
                (z_near, vec3f(0., 0., 0.), vec3f(0., 0., 0.)),
                (z_near, vec3f(1., 0., 0.), vec3f(0., 0., 0.)),
                (z_near, vec3f(-1., 0., 0.), vec3f(0., 0., 0.)),
                (z_near, vec3f(0., 1., 0.), vec3f(0., 0., 0.)),
                (z_near, vec3f(0., -1., 0.), vec3f(0., 0., 0.)),
                (z_mid, vec3f(0., 0., 0.), vec3f(0., 0., 0.)),
                (z_mid, vec3f(0., 0., 0.), vec3f(-0.3, -0.2, 0.)),
                (z_mid, vec3f(0., 0., 0.), vec3f(-0.3, 0.2, 0.)),
                (z_mid, vec3f(0., 0., 0.), vec3f(0.3, -0.2, 0.)),
                (z_mid, vec3f(0., 0., 0.), vec3f(0.3, 0.2, 0.)),
            ];

            let variant_i = (config.current_frame() % (variants.len() as u64)) as usize;

            let (z, angle, offset) = variants.remove(variant_i);

            let mut t = Matrix4f::identity();

            // Center the checkerboard.
            t = translate(&vec3f(
                -grid_width / 2.0,
                -grid_height / 2.0,
                0.0
            )) * t;

            // Make the grid horizontal.
            if grid_width < grid_height {
                t = rotate(&(vec3f(
                    0.,
                    0.,
                    1.
                ) * (PI / 2.0))) * t;
            }

            // tilt
            t = rotate(&(angle * (PI / 6.0))) * t;

            // Move back.
            t = translate(&vec3f(0., 0., z)) * t;

            t = translate(&offset) * t;

            scene.checkerboard = Some(t);
        }

        if config.wanding_calibration() {

            let num_dup_frames = 4;

            let mut rng = MersenneTwisterRng::mt19937();
            rng.seed_u32((config.current_frame() / num_dup_frames) as u32);

            let translation = vec3d(
                rng.between::<f64>(-1.0, 1.0),
                rng.between::<f64>(-1.0, 1.0),
                rng.between::<f64>(0.0, 1.0),
            );

            let rotation = {
                use std::f64::consts::PI;

                let u_1 = rng.between::<f64>(0.0, 1.0);
                let u_2 = rng.between::<f64>(0.0, 1.0);
                let u_3 = rng.between::<f64>(0.0, 1.0);

                let mut q_w = (1. - u_1).sqrt() * (2. * PI * u_2).sin();
                let mut q_x = (1. - u_1).sqrt() * (2. * PI * u_2).cos();
                let mut q_y = u_1.sqrt() * (2. * PI * u_3).sin();
                let mut q_z = u_1.sqrt() * (2. * PI * u_3).cos();

                if q_w < 0.0 {
                    q_w *= -1.0;
                    q_x *= -1.0;
                    q_y *= -1.0;
                    q_z *= -1.0;
                }

                let angle = 2.0 * q_w.acos();

                let axis = {
                    let scale = (1.0 - q_w * q_w).sqrt();
                    if scale < 0.0001 {
                        vec3d(1.0, 0.0, 0.0)
                    } else {
                        vec3d(
                            q_x / scale,
                            q_y / scale,
                            q_z / scale,
                        )
                    }
                };

                from_axis_angle(&(axis * angle))
            };

    
            for pt in shared.wand_points.iter().cloned() {
                let p = (&rotation * pt) + &translation;

                scene.spheres.push(Sphere {
                    center: p.cast(),
                    radius: 0.01
                });
            }


        }

        if config.spinning_wand() {
            use std::f64::consts::PI;

            let interval = 1000;
            let percent = ((config.current_frame() % interval) as f64) / (interval as f64);

            let angle = (2.0 * PI) * percent;

            let z = {
                if percent < 0.5 {
                    percent / 0.5
                } else {
                    (1. - percent) / 0.5
                }
            };

            let translation = vec3d(0., 0., 0.5 * z);
            let rotation = from_axis_angle(&vec3d(0., 0., angle));


            for pt in shared.wand_points.iter().cloned() {
                let p = (&rotation * pt) + &translation;

                scene.spheres.push(Sphere {
                    center: p.cast(),
                    radius: 0.01
                });
            }
        }

        scene
    }
}

fn translate(translation: &Vector3f) -> Matrix4f {
    let mut out = Matrix4f::identity();
    out.block_mut(0, 3).copy_from(translation);
    out
} 

fn rotate(rotation: &Vector3f) -> Matrix4f {
    let mut out = Matrix4f::identity();
    out.block_mut(0, 0).copy_from(&from_axis_angle(&rotation.cast()).cast());
    out
}

struct RendererThread {
    shared: Arc<Shared>,
    renderer: MocapFrameRenderer,
    frame_processor: FrameProcessor,
    last_rendered: Option<RenderedResult>,
    jpeg_encoder: JPEGEncoder,
}

#[derive(PartialEq, Clone)]
struct RenderRequest {
    scene: MocapCameraRendererScene,
    camera_settings: HashMap<u64, CameraSettings, FastHasherBuilder>
}

#[derive(Debug, PartialEq, Clone)]
struct CameraSettings {
    blob_filter: BlobFilterConfig,
    blob_threshold: u8,
    running: bool,
}

struct RenderedResult {
    request: RenderRequest,
    blobs: Vec<ReadBlobsResponse>,
    frames: Vec<(u64, Image<u8>)>,
    encoded_frames: Vec<(u64, Bytes)>,
}

impl RendererThread {
    fn render_once(&mut self, request: &RenderRequest) -> Result<Vec<ReadBlobsResponse>> {
        // TODO: Caching will need to factor in all the settings.
        if let Some(last) = &self.last_rendered {
            if &last.request == request {
                return Ok(last.blobs.clone());
            }
        }

        // TODO: We can pipeline the current frame processing and new frmae rendering.
        // (or when one frame is done, we can start processing it.)
        // 
        // TODO: Don't render cameras that are at 0 FPS.
        let frames = self.renderer.render(&request.scene)?;

        // TODO: Parallelize / pipeline this (probably can split out of the RenderThread struct).
        let mut out = vec![];
        for (camera_id, image) in &frames {

            let settings = request.camera_settings.get(camera_id).unwrap();

            if !settings.running {
                continue;
            }

            let r = self.frame_processor.process(&image.array.data, settings.blob_threshold, &settings.blob_filter);

            let mut results = ReadBlobsResponse::default();
            let p = results.new_cameras();
            p.set_results(r);
            p.set_camera_id(*camera_id);

            out.push(results);
        }

        self.last_rendered = Some(RenderedResult {
            request: request.clone(),
            blobs: out.clone(),
            frames,
            encoded_frames: vec![]
        });

        Ok(out)
    }

    fn get_encoded_frame(&mut self, camera_id: u64) -> Result<Option<Bytes>> {
        let res = self.last_rendered.as_mut().unwrap();

        if let Some((_, existing)) = res.encoded_frames.iter().find(|(id, _)| *id == camera_id) {
            return Ok(Some(existing.clone()));
        }
        
        let (_, image) = match res.frames.iter().find(|(id, _)| *id == camera_id) {
            Some(v) => v,
            None => return Ok(None)
        };

        let image_ref = image::ImageRef {
            width: image.width(),
            height: image.height(),
            channels: image.channels(),
            data: &image.array.data
        };

        let mut data = vec![];
        self.jpeg_encoder.encode_raw(&image_ref, &mut data)?;

        let data: Bytes = data.into();
        res.encoded_frames.push((camera_id, data.clone()));

        Ok(Some(data))
    }

}



/// This is mainly to extract data from the 'MocapSimulator' for a specific camera and send it over RPC 
struct SimulatedMocapCamera {
    camera_id: u64,
    shared: Arc<Shared>,
}

#[async_trait]
impl MocapCameraService for SimulatedMocapCamera {

    async fn Status(
        &self,
        request: rpc::ServerRequest<StatusRequest>,
        response: &mut rpc::ServerResponse<MocapCameraStatus>
    ) -> Result<()> {
        let cam = self.shared.cameras.get(&self.camera_id).unwrap();
        response.value = cam.status.apply(|s| s.clone())?;
        Ok(())
    }

    async fn Configure(
        &self,
        request: rpc::ServerRequest<MocapCameraConfigureRequest>,
        response: &mut rpc::ServerResponse<ConfigureResponse>
    ) -> Result<()> {
        let cam = self.shared.cameras.get(&self.camera_id).unwrap();

        cam.status.apply(|status| {
            let mut config = request.value.clone();

            // TODO: Implement partial merging
            if !config.has_camera_controls() {
                config.set_camera_controls(status.config().camera_controls().clone());
            }

            status.set_config(config);
        })?;

        Ok(())
    }


    // TODO: Dedup this code with the real camera.
    async fn ReadBlobs(
        &self,
        request: rpc::ServerRequest<ReadBlobsRequest>,
        response: &mut rpc::ServerStreamResponse<ReadBlobsResponse>
    ) -> Result<()> {
        let cam = self.shared.cameras.get(&self.camera_id).unwrap();

        // TODO: Need some logging if we ever drop frames
        let mut subscriber = cam.blob_subscribers.subscribe(1024);

        response.send_head().await?;

        let mut last_response = Instant::now();

        let mut min_interval = Duration::ZERO;
        if request.max_rate() != 0 {
            min_interval = Duration::from_secs_f32(1.0 / (request.max_rate() as f32));
        }

        loop {
            let res = subscriber.recv().await?;
            
            let now = Instant::now();
            if now - last_response >= min_interval {
                response.send(res).await?;
                last_response = now;
            }
        }

        Ok(())
    }

    async fn ReadFrames(
        &self,
        request: rpc::ServerRequest<ReadFramesRequest>,
        response: &mut rpc::ServerStreamResponse<ReadFramesResponse>
    ) -> Result<()> {
        let cam = self.shared.cameras.get(&self.camera_id).unwrap();

        let mut subscriber = cam.mjpeg_subscribers.subscribe(8);

        loop {
            let frame = subscriber.recv().await?;

            let mut res = ReadFramesResponse::default();
            // TODO: Make this zero copy
            res.set_mjpeg(&frame[..]);
            response.send(res).await?;
        }

        Ok(())
    }


    async fn FlashMCU(
        &self,
        request: rpc::ServerRequest<FlashMCURequest>,
        response: &mut rpc::ServerResponse<FlashMCUResponse>
    ) -> Result<()> {
        Ok(())
    }
}
