use common::errors::*;
use mocap_proto::mocap::*;
use vision::connected_components::*;

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
            max_aspect_ratio: 1.5
            circular_area_error: 0.25
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

            if aspect_ratio > blob_filter.max_aspect_ratio() {
                // println!("Reject aspect ratio: {}", aspect_ratio);
                out.reject_component(&component);
                continue;
            }

            let radius_x = (bbox_width as f32) / 2.0;
            let radius_y = (bbox_height as f32) / 2.0;

            let expected_area = std::f32::consts::PI * radius_x * radius_y;
            let area_error = (expected_area - (component.area as f32)) / expected_area;
            if area_error.abs() > blob_filter.circular_area_error() {
                // println!("Reject circular area");
                out.reject_component(&component);
                continue;
            }

            let mut blob = Blob::default();
            blob.set_x(((component.moment_x as f32) / (component.mass as f32)) + 0.5);
            blob.set_y(((component.moment_y as f32) / (component.mass as f32)) + 0.5);
            blob.set_radius((radius_x + radius_y) / 2.0);
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



