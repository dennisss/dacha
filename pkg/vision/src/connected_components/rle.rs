use crate::connected_components::disjoint_sets::*;
use crate::connected_components::component::*;
use crate::connected_components::simd::*;


/// Run Length Encoding style connected components analyzer.
/// Uses 8-way connectivity.
///
/// This uses a single-pass algorithm which is suitable for progressive scanning of an image.
///
/// This extracts component stats from an image frame as follows:
/// - For each line in the image
///   - Convert the line into a list of {start_offset, num_pixels, stats} runs of consecutive
///     pixels that are above the matching threshold.
///     - Note that we are aggregating stats for each pixel as we build the runs.
///     - Each time a line is completed, we merge it with all sets of the previous/last line
///       that overlap with it (this involves copying the previous run's label or merging them
///       via a disjoint sets datastructure)
///   - Save the list of runs as the 'last_line' runs for merging later.
/// - Once done, extract the final set of merged components from the disjoint sets
///   datastructure.
///
/// The 'merge with previous line' steps are pretty efficient since this uses merge sort style zipping
/// of sorted lists of runs.
///
/// Compared to SAUF, this approach is likely to produce fewer 'sets' since we only check for existing
/// sets on entire runs before we decide to generate a new distinct set for a set of pixels, but each
/// individual set will consume more memory during intermediate calculations since we store the full
/// set of ComponentData statistics
///
/// Compared to SAUF, we also don't need the full labels array for the entire image will 'probably'
/// save memory since we replace it with a list of runs for the current and previous line. This should
/// result in big memory savings for very sparse images.
pub struct RLEConnectedComponentsProcessor {
    image_width: usize,
    image_height: usize,

    num_lines_processed: usize,

    // NOTE: This will contain some stale data since when we merge sets, old data for one of the sets
    // stays in here but there are no references to it in component_sets
    components: Vec<ComponentData>,

    component_sets: DisjointSets,
    last_line: Vec<PixelRun>,
    last_line_run_i: usize,
    
    current_line: Vec<PixelRun>,
    current_run_component: ComponentData,
}

#[derive(Clone)]
struct PixelRun {
    /// x index of the first pixel in this run.
    x_first: u16,
    
    /// x index of the last pixel in this run
    x_last: u16,

    // Note that this is not directly an index into 'components' but a reference to a
    // set in 'component_sets' whose root is the index of the entry in 'components'.
    component_id: u32,
}

impl RLEConnectedComponentsProcessor {

    pub fn new(image_width: usize, image_height: usize) -> Self {
        // Required for SIMD.
        assert!(image_width % SIMD_THRESHOLDING_CHUNK_SIZE == 0);

        let mut components = vec![];
        components.reserve(128);

        let mut component_sets = DisjointSets::default();
        component_sets.reserve(128);

        Self {
            image_width,
            image_height,
            num_lines_processed: 0,
            components,
            component_sets,
            last_line: vec![],
            last_line_run_i: 0,
            current_line: vec![],
            current_run_component: ComponentData::empty(),
        }
    }

    pub fn process<'a>(&'a mut self, frame: &[u8], threshold: u8) -> &'a [ComponentData] {
        self.reset();
        self.process_lines(frame, threshold);
        self.finish()
    }

    pub fn reset(&mut self) {
        self.num_lines_processed = 0;
    }

    #[inline(never)]
    pub fn process_lines(&mut self, frame: &[u8], threshold: u8) {
        assert!(frame.len() % self.image_width == 0);
        let num_lines = frame.len() / self.image_width;

        let end_line = num_lines + self.num_lines_processed;
        assert!(end_line <= self.image_height);  

        if self.num_lines_processed == 0 {
            self.components.clear();
            self.component_sets.clear();
            self.last_line.clear();
        }

        let mut offset = 0;

        for y in self.num_lines_processed..end_line {
            self.current_line.clear();
            self.last_line_run_i = 0;

            let mut x = 0;
            while x < self.image_width {
                let chunk = array_ref![frame, offset, SIMD_THRESHOLDING_CHUNK_SIZE];
                if !is_any_greater_than_threshold(chunk, threshold) {
                    x += SIMD_THRESHOLDING_CHUNK_SIZE;
                    offset += SIMD_THRESHOLDING_CHUNK_SIZE;
                    continue;
                }

                for chunk_i in 0..SIMD_THRESHOLDING_CHUNK_SIZE {
                    let v = unsafe  { *chunk.get_unchecked(chunk_i) };

                    if v <= threshold {
                        x += 1;
                        continue;
                    }

                    self.advance_current_run(x, y);

                    let intensity = (v - threshold) as u64;

                    // self.current_run_component.add_pixel(x, y, intensity);
                    self.current_run_component.add_row_pixel(x, y, intensity);

                    x += 1;
                }
                offset += SIMD_THRESHOLDING_CHUNK_SIZE;
            }

            if self.current_run_component.area != 0 {
                self.finalize_run();
            }

            core::mem::swap(&mut self.current_line, &mut self.last_line);
        }

        self.num_lines_processed = end_line;
    }

    pub fn finish<'a>(&'a mut self) -> &'a [ComponentData] {
        assert_eq!(self.num_lines_processed, self.image_height);
        
        let mut i = 0;
        self.components.retain(|_| {
            let keep = self.component_sets.is_root(i);
            i += 1;
            keep
        });

        self.num_lines_processed = 0;

        &self.components[..]
    }

    #[inline(always)]
    fn advance_current_run(&mut self, x: usize, y: usize) {
        if self.current_run_component.area != 0 {
            // Continue the current run.
            if self.current_run_component.max_x + 1 == (x as u16) {
                return;
            }
            
            // Finalize the current run
            self.finalize_run();
        }

        self.current_run_component.start_pixel_row(x, y);
    }

    // TODO: Consider allowing skipping of 1-pixel wide runs since they aren't very useful and would halve the
    // worse case computation per pixel.
    fn finalize_run(&mut self) {
        self.current_run_component.finish_pixel_row();
        
        let mut run = PixelRun {
            x_first: self.current_run_component.min_x,
            x_last: self.current_run_component.max_x,
            component_id: 0
        };

        // This is the complete inclusive range of x values that are considered to be overlapping with
        // the current run based on 8-way connectivity.
        let min_connected_x = (run.x_first as isize) - 1;
        let max_connected_x = (run.x_last as isize) + 1;

        let mut have_component = false;

        let mut cur_last_line_run_i = self.last_line_run_i;

        while cur_last_line_run_i < self.last_line.len() {

            let last_line_run = &self.last_line[cur_last_line_run_i];

            // Skip last_line_run if it is completely before the current run.
            //
            // NOTE: This check should only get triggered for the starting iterations of the outer
            // loop. After that, the runs should all be matching until we hit the stop condition.
            // let last_line_last_x = (last_line_run.x_first + last_line_run.length) as i16 - 1;
            if (last_line_run.x_last as isize) < min_connected_x {
                // Run is completely before this line.
                self.last_line_run_i += 1;
                cur_last_line_run_i += 1;
                continue;
            }

            // Stop if last_line_run is completely beyond our current run.
            if (last_line_run.x_first as isize) > max_connected_x {
                // Last run line is completely beyond the current run. 
                break;
            }

            // Else, the runs intersect so need to be merged

            let last_component_i = self.component_sets.find(last_line_run.component_id as usize);

            if !have_component {
                // Copy the component from the last line's run since we don't have a real
                // component for the current run yet.
                //
                // TODO: This line can be optimized quite a bit:
                // - We should never need to update y_min and we can just always update y_max.
                // - We can run the computation in finish_pixel_row() at the same time as doing this add.
                self.components[last_component_i].add(&self.current_run_component);
                run.component_id = last_component_i as u32;
                have_component = true;
                cur_last_line_run_i += 1;
                continue;
            }

            let cur_component_i = self.component_sets.find(run.component_id as usize);

            if last_component_i != cur_component_i {
                
                let merged_root = self.component_sets.union(last_component_i, cur_component_i);

                let new_root_i = cur_component_i.min(last_component_i);
                // assert_eq!(new_root_i, merged_root);
                let old_data_i = cur_component_i.max(last_component_i);

                unsafe {
                    let ptr = self.components.as_mut_ptr();

                    let old_data = &*ptr.add(old_data_i);
                    let new_root = &mut *ptr.add(new_root_i);

                    new_root.add(old_data);
                }
            }

            cur_last_line_run_i += 1;
        }

        if !have_component {
            let set_id = self.component_sets.new_set();
            // assert_eq!(set_id, self.components.len());
            self.components.push(self.current_run_component.clone());
            run.component_id = set_id as u32;
        }

        self.current_run_component = ComponentData::empty();

        self.current_line.push(run);
    }

}
