use math::matrix::{Vector2f, vec2f};

#[derive(Clone, Debug)]
pub struct CameraIntrinsicsModel {
    /// Currently assuming it is the same for x and y
    pub focal_length: f32,

    pub center: Vector2f,
}

impl CameraIntrinsicsModel {
    pub fn from_nominal_params(
        frame_width: usize,
        frame_height: usize,
        focal_length: f32,
        pixel_size: f32
    ) -> Self {
        let center = vec2f((frame_width as f32) / 2.0, (frame_height as f32) / 2.0);
        let focal_length = focal_length / pixel_size;

        Self {
            focal_length,
            center
        }
    }

}

pub fn millis(v: f32) -> f32 {
    v / 1_000.0
}

pub fn micros(v: f32) -> f32 {
    v / 1_000_000.0
}