use crate::connected_components::disjoint_sets::*;
use crate::connected_components::component::*;
use crate::connected_components::simd::*;

/// Based on the Scan Array Union Find algorithm (https://sdm.lbl.gov/~kewu/ps/paa-final.pdf)
/// Uses 8-way connectivity.
pub struct SAUFConnectedComponentsProcessor {
    image_width: usize,
    image_height: usize,

    /// For each image in the pixel currently being processed, this is an index
    /// of a set in label_sets (only valid for pixels > threshold).
    ///
    /// NOTE: We don't need to zero this between frames since we always use the
    /// frame's pixel value to determine if a pixel is present before reading
    /// from this list.
    labels: Vec<u32>,

    label_sets: DisjointSets,

    components: Vec<ComponentData>,
}

impl SAUFConnectedComponentsProcessor {
    pub fn new(image_width: usize, image_height: usize) -> Self {
        let mut label_sets = DisjointSets::default();
        label_sets.reserve(128);
        
        Self {
            image_width,
            image_height,
            labels: vec![0; image_width * image_height],
            label_sets: DisjointSets::default(),
            components: vec![],
        }
    }

    // TODO: Consider returning a reference to internal memory.
    #[inline(never)]
    pub fn process<'a>(&'a mut self, frame: &[u8], threshold: u8) -> &'a [ComponentData] {
        self.reset();
        self.process_lines(frame, threshold);
        self.finish()
    }

    pub fn reset(&mut self) {

    }

    pub fn process_lines(&mut self, frame: &[u8], threshold: u8) {
        assert_eq!(frame.len(), self.image_height * self.image_width);

        self.label_sets.clear();

        // First pass: form sets of all pixels in the same component.
        Self::iter_pixels_thresholded(frame, threshold, |i| {
            let have_above = i >= self.image_width;
            let col = i % self.image_width;
            let have_left = col > 0;
            let have_right = col < (self.image_width - 1);

            // 'Decision tree 1' from the SAUF paper

            if have_above {
                let b_i = i - self.image_width;
                let b = frame[b_i] > threshold;
                if b {
                    self.labels[i] = self.labels[b_i];
                    return;
                }

                if have_right {
                    let c_i = b_i + 1;
                    let c = frame[c_i] > threshold;
                    if c {

                        if have_left {
                            let a_i = b_i - 1;
                            let a = frame[a_i] > threshold;
                            if a {
                                // copy(c, a)
                                let a_label = self.labels[a_i] as usize;
                                let c_label = self.labels[c_i] as usize;
                                self.labels[i] = self.label_sets.union(a_label, c_label) as u32;
                                return;
                            }

                            let d_i = i - 1;
                            let d = frame[d_i] > threshold;
                            if d {
                                // copy(c,d)
                                let d_label = self.labels[d_i] as usize;
                                let c_label = self.labels[c_i] as usize;
                                self.labels[i] = self.label_sets.union(d_label, c_label) as u32;
                                return;
                            }
                        }

                        self.labels[i] = self.labels[c_i];

                        return;
                    }
                }

                // If we are here, b,c == 0

                if have_left {
                    let a_i = b_i - 1;
                    let a = frame[a_i] > threshold;
                    if a {
                        self.labels[i] = self.labels[a_i];
                        return;
                    }
                }
            }

            // If we are here, then a,b,c == 0

            if have_left {
                let d_i = i - 1;
                let d = frame[d_i] > threshold;
                if d {
                    self.labels[i] = self.labels[d_i];
                    return;
                }
            }
            
            // No neighbors are matching.
            self.labels[i] = self.label_sets.new_set() as u32; 
        });

        let num_components = self.label_sets.flatten_and_relabel();

        self.components.clear();
        self.components.resize(num_components, ComponentData::empty());

        // Second pass: extract components with metrics.
        Self::iter_pixels_thresholded(frame, threshold, |i| {
            let intensity = (frame[i] - threshold) as u64;
            let x = i % self.image_width;
            let y = i / self.image_width;

            let component_i = self.label_sets.parents[self.labels[i] as usize] as usize;
            let component = &mut self.components[component_i];
            component.add_pixel(x, y, intensity);
        });
    }

    pub fn finish<'a>(&'a mut self) -> &'a [ComponentData] {
        &self.components
    }


    #[inline(always)]
    fn iter_pixels_thresholded<F: FnMut(usize)>(frame: &[u8], threshold: u8, mut f: F) {

        // Unoptimized equivalent code
        /*
        for i in 0..frame.len() {
            if frame[i] <= threshold {
                continue;
            }

            f(i);
        }
        */

        let mut i = 0;
        for chunk in frame.chunks_exact(SIMD_THRESHOLDING_CHUNK_SIZE) {
            if !is_any_greater_than_threshold(unsafe { core::mem::transmute(chunk.as_ptr()) }, threshold) {
                i += SIMD_THRESHOLDING_CHUNK_SIZE;
                continue;
            }

            for _ in 0..SIMD_THRESHOLDING_CHUNK_SIZE {
                if frame[i] <= threshold {
                    i += 1;
                    continue;
                }

                f(i);

                i += 1;
            }

        }
    }

}