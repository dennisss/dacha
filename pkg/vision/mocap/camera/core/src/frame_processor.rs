use common::errors::*;
use mocap_proto::mocap::*;
use vision::connected_components::*;
use math::matrix::vec2f;

type ConnectedComponentsProcessor = RLEConnectedComponentsProcessor;

/// Processes camera frames seen by a mocap camera.
///
/// Pipeline is:
/// - Apply minimum intensity threshold (fused with the next step)
/// - Find connected components.
/// - Filter out non-circular / small components.
/// - Rank components and return top-k
pub struct FrameProcessor {
    image_width: usize,
    image_height: usize,
    cc_processor: ConnectedComponentsProcessor,
}

impl FrameProcessor {
    pub fn default_blob_filter_config() -> Result<BlobFilterConfig> {
        let mut blob_filter = BlobFilterConfig::default();
        protobuf::text::parse_text_proto(r#"
            min_area: 6
            max_area: 6000
            reject_edge: true
            max_bbox_aspect_ratio: 1.5
            elliptical_area_error: 0.25
            max_radius_ratio: 1.5
            max_centroid_bbox_skew {
                abs: 2.5
                rel: 0.05
            }
        "#, &mut blob_filter)?;
        Ok(blob_filter)
    }

    pub fn new(image_width: usize, image_height: usize) -> Self {
        Self {
            image_width,
            image_height,
            cc_processor: ConnectedComponentsProcessor::new(image_width, image_height)
        }
    }

    /*
    TODO: Need to support stopping early if we are running over time.
    */

    #[inline(never)]
    pub fn process(
        &mut self,
        frame: &[u8],
        threshold: u8,
        blob_filter: &BlobFilterConfig
    ) -> BlobResults {
        self.reset();
        self.process_lines(frame, threshold);
        self.finish(blob_filter)
    }

    pub fn reset(&mut self) {
        self.cc_processor.reset();
    }

    #[inline(never)]
    pub fn process_lines(&mut self, frame: &[u8], threshold: u8) {
        self.cc_processor.process_lines(frame, threshold);
    }

    pub fn finish(&mut self, blob_filter: &BlobFilterConfig) -> BlobResults {

        let components = self.cc_processor.finish();

        let mut out = BlobResultsBuilder::default();

        for component in components {
            if component.area < blob_filter.min_area() {
                // println!("Reject min area");
                out.reject_component(&component);
                continue;
            }

            if component.area > blob_filter.max_area() {
                // println!("Reject max area: {}", component.area);
                out.reject_component(&component);
                continue;
            }

            if blob_filter.reject_edge() {
                if component.min_x == 0 || component.min_y == 0 ||
                    component.max_x == (self.image_width - 1) as u16 ||
                    component.max_y == (self.image_height - 1) as u16
                {
                    // println!("Reject edge filter");
                    out.reject_component(&component);
                    continue;
                }
            }

            let bbox_width = ((component.max_x - component.min_x) as usize) + 1;
            let bbox_height = ((component.max_y - component.min_y) as usize) + 1;

            let mut aspect_ratio = (bbox_width as f32) / (bbox_height as f32);
            if aspect_ratio < 1.0 {
                aspect_ratio = 1.0 / aspect_ratio;
            }

            if aspect_ratio > blob_filter.max_bbox_aspect_ratio() {
                // println!("Reject aspect ratio: {}", aspect_ratio);
                out.reject_component(&component);
                continue;
            }

            let stats = component.calculate_stats();

            if stats.radius_a / stats.radius_b > blob_filter.max_radius_ratio() {
                out.reject_component(&component);
                continue;
            }

            let expected_area = std::f32::consts::PI * stats.radius_a * stats.radius_b;
            let area_error = (expected_area - (component.area as f32)) / expected_area;
            if area_error.abs() > blob_filter.elliptical_area_error() {
                // println!("Reject circular area");
                out.reject_component(&component);
                continue;
            }

            let mass_centroid = vec2f(stats.centroid_x, stats.centroid_y);
            let bbox_centroid = vec2f(
                (component.min_x as f32) + ((bbox_width as f32) / 2.0),
                (component.min_y as f32) + ((bbox_height as f32) / 2.0),
            );

            let max_centroid_error = (
                blob_filter.max_centroid_bbox_skew().abs().max(
                    blob_filter.max_centroid_bbox_skew().rel() * (stats.radius_a.max(stats.radius_b) as f32)
                )
            );

            if (mass_centroid - bbox_centroid).norm_squared() > (max_centroid_error * max_centroid_error) {
                out.reject_component(&component);
                continue;
            }

            let mut blob = Blob::default();
            blob.set_x(stats.centroid_x);
            blob.set_y(stats.centroid_y);
            blob.set_radius_a(stats.radius_a);
            blob.set_radius_b(stats.radius_b);
            blob.set_angle(stats.angle);
            out.add_blob(blob);
        }

        out.finish()
    }

    
}




#[derive(Default)]
struct BlobResultsBuilder {
    out: BlobResults, 
}

impl BlobResultsBuilder {

    fn add_blob(&mut self, blob: Blob) {
        self.out.add_blobs(blob);
    }

    fn reject_component(&mut self, component: &ComponentData) {
        // println!("{:?}", component);
        
        {
            let v = self.out.num_rejected_blobs();
            self.out.set_num_rejected_blobs(v + 1);
        }

        {
            let v = self.out.num_rejected_pixels(); 
            self.out.set_num_rejected_pixels(v + component.area);
        }
    }

    fn finish(self) -> BlobResults {
        self.out
    }
}



