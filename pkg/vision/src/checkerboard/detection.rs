

/*
Based on ROCHADE
https://www5.cs.fau.de/fileadmin/research/Publikationen/2014/Placht14-RRC.pdf

Note that this internally uses an 'integer center' coordinate system for calculations but
the final reuslts are corners in 'integer corner' coordinates.
*/

use std::time::{Instant, Duration};
use std::collections::{HashSet, HashMap};

use common::hash::FastHasherBuilder;
use image::{Image, Colorspace, Color};
use math::array::{Array, KernelEdgeMode};
use math::matrix::{Vector2f, vec2f, vec3f, MatrixXf, Vector3f, vec2d, Vector2d};

use crate::checkerboard::refinement::*;
use crate::checkerboard::utils::*;
use crate::checkerboard::drawing::*;

const LOCAL_THRESHOLDING_RADIUS: isize = 4;

const LOCAL_THRESHOLDING_RELATIVE_THRESHOLD: f32 = 0.4; // top 60% of intensities

// This is in pixel intensity units (0-255 scale)
const LOCAL_THRESHOLDING_ABSOLUTE_THRESHOLD: f32 = 2.0;

// NOTE: This is '6' in the original Rochade paper.
const DILATION_MIN_NEIGHBORS: usize = 4;

const SADDLE_POINT_MERGE_RADIUS: f32 = 5.0;




fn time_work<T, F: FnOnce() -> T>(name: &str, mut f: F) -> T {
    let a = Instant::now();
    let ret = f();
    let b = Instant::now();
    println!("{} took {:?}", name, b - a);
    ret
}

#[derive(Default)]
pub struct CheckerboardDetectionResult {
    pub debug_images: Vec<Image<u8>>,

    pub points: Option<Vec<Vector2d>>,
}

#[derive(Default)]
pub struct CheckerboardDetectionOptions {
    pub grid_width: usize,
    pub grid_height: usize,
    pub output_debug_images: bool
}

pub async fn detect_checkboard(
    img: &Image<u8>, options: &CheckerboardDetectionOptions
) -> CheckerboardDetectionResult {

    let mut res = CheckerboardDetectionResult::default();

    assert_eq!(img.channels(), 1);

    let arr = img.array()
        .reshape(&[ img.height(), img.width() ]).cast::<f32>();

    let gradients = time_work("Scharr", || {
        scharr_filter(&arr)
    });

    if options.output_debug_images {
        res.debug_images.push(Image {
            array: gradients.cast(),
            colorspace: Colorspace::Grayscale
        });
    }

    let thresholded = time_work("LocalThresholding", || {
        apply_local_thresholding(&gradients)
    });

    let dilated = time_work("Dilation", || apply_condional_dilation(&thresholded));

    if options.output_debug_images {
        res.debug_images.push(Image {
            array: dilated.cast(),
            colorspace: Colorspace::Grayscale
        });
    }

    let background_distances = time_work("DistanceTransform", || calculate_distance_transform(&dilated));

    let mut centerline = time_work("Centerline", || find_centerline(&dilated, &background_distances));

    time_work("Cleaning", || {
        prune_dead_ends(&mut centerline);
        merge_redundant_pixels(&mut centerline);
        prune_dead_ends(&mut centerline);
        merge_redundant_pixels(&mut centerline);
    });


    if options.output_debug_images {
        res.debug_images.push(Image {
            array: centerline.cast(),
            colorspace: Colorspace::Grayscale
        });
    }

    let saddle_points = time_work("FindSaddle", || find_saddle_points(&centerline));

    let saddle_clusters = time_work("MergeSaddle", move || merge_saddle_points(saddle_points, SADDLE_POINT_MERGE_RADIUS));

    // println!("Num Saddle clusters: {}", saddle_clusters.len());

    let saddle_edges = time_work("AdjacentClusters", || find_adjacent_clusters(&centerline, &saddle_clusters));

    let saddle_components = time_work("Connect", || connect_cluster_components(&saddle_clusters, &saddle_edges));


    let mut found_geometry = None;
    for component in saddle_components {
        if component.clusters.len() != options.grid_height * options.grid_width {
            continue;
        }

        // println!("Found component!");

        let mut geometry = match check_checkerboard_geometry(&saddle_clusters, &saddle_edges, &component) {
            Some(v) => v,
            None => continue
        };

        if geometry.width != options.grid_width {
            geometry = geometry.transpose();
        }

        if geometry.width != options.grid_width || geometry.height != options.grid_height {
            continue;
        }

        found_geometry = Some(geometry);
        // println!("Found it!");
        break;
    }

    let mut geometry = match found_geometry {
        Some(v) => v,
        None => return res
    };

    // With x coordinate increasing along the width and y increasing along height,
    // ensure the checkerboard is not flipped upside down (z should be increasing away from the camera)
    {
        let pt_0 = saddle_clusters[geometry.clusters[0]].average_point();
        let pt_x = saddle_clusters[geometry.clusters[1]].average_point();
        let pt_y = saddle_clusters[geometry.clusters[options.grid_width]].average_point();

        let x_vec = pt_x - &pt_0;
        let y_vec = pt_y - &pt_0; 

        let z_vec = vec3f(x_vec.x(), x_vec.y(), 0.0).cross(&vec3f(y_vec.x(), y_vec.y(), 0.0));
    
        if z_vec.z() < 0.0 {
            geometry = geometry.flip_x();
        }
    }

    // Normalize the first first point to be on the left side
    // (basically a 180 degree rotation)
    {
        let pt_first = saddle_clusters[geometry.clusters[0]].average_point();
        let pt_last = saddle_clusters[geometry.clusters[geometry.clusters.len() - 1]].average_point();

        if pt_last.x() < pt_first.x() {
            geometry.clusters.reverse();
        }
    }


    // TODO: Also make this conditionally generated.
    let mut debug_img = gray_to_color(img);

    if options.output_debug_images {

        for cluster_idx in geometry.clusters.iter().cloned() {
            let pt = saddle_clusters[cluster_idx].average_point();
            draw_color_circle(&pt, &Color::rgb(0xff, 0, 0), &mut debug_img);
        }
        res.debug_images.push(debug_img.clone());
    }

    let mut refined_points = vec![]; 

    time_work("Refine", || {
        
        let refiner = SubpixelCornerRefiner::new(&arr);

        for cluster_idx in geometry.clusters.iter().cloned() {
            let pt = saddle_clusters[cluster_idx].average_point();

            let pt = match refiner.refine_corner(&pt) {
                Some(v) => v,
                None => {
                    println!("=> Refine FAILED");
                    break;
                }
            };

            refined_points.push(pt);
        }


    });


    if options.output_debug_images {
        for pt in refined_points.iter().cloned() {
            draw_color_circle(&pt, &Color::rgb(0, 0xff, 0), &mut debug_img);
        }
        res.debug_images.push(debug_img.clone());
    }

    if refined_points.len() == geometry.clusters.len() {
        res.points = Some(refined_points.into_iter().map(|p| p.cast() + vec2d(0.5, 0.5)).collect());
    }

    res
}

fn scharr_filter(image: &Array<f32>) -> Array<f32> {
    let filter_x = Array::<f32>::from_slice(&[
        -3.0, 0.0, 3.0,
        -10.0, 0.0, 10.0,
        -3.0, 0.0, 3.0
    ]).reshape(&[3, 3]);

    let filter_y = Array::<f32>::from_slice(&[
        -3.0, -10.0, -3.0,
        0.0, 0.0, 0.0,
        3.0, 10.0, 3.0
    ]).reshape(&[3, 3]);

    let gx = cross_correlate_fast_2d(&image, &filter_x);
    let gy = cross_correlate_fast_2d(&image, &filter_y);

    // '/ 32.0' keeps the image normalized to the input pixel scale.
    let g_mag = gx.zip(&gy, |x, y| (squared(x) + squared(y)).sqrt() / 32.0);

    g_mag
}

fn squared(v: f32) -> f32 {
    v * v
}

fn cross_correlate_fast_2d(input: &Array<f32>, kernel: &Array<f32>) -> Array<f32> {
    assert_eq!(input.shape.len(), 2);
    assert_eq!(kernel.shape.len(), 2);

    let mut out = Array {
        data: vec![0.0f32; input.data.len()],
        shape: input.shape.clone()
    };

    let height = input.shape[0];
    let width = input.shape[1];

    let kernel_size = kernel.shape[0];
    assert_eq!(kernel_size % 2, 1);
    assert_eq!(kernel.shape[1], kernel_size);

    let kernel_width = kernel_size / 2;

    for y in kernel_width..(height - kernel_width) {
        for x in kernel_width..(width - kernel_width) {
            let i = y * width + x;

            let mut sum = 0.0;

            let mut j = i - width * kernel_width - kernel_width;
            let mut k = 0;
            for _ in 0..kernel_size {
                for _ in 0..kernel_size {
                    sum += input.data[j] * kernel[k];
                    j += 1;
                    k += 1;
                }

                j += width - kernel_size;
            }

            out.data[i] = sum;
        }
    }

    out
} 


fn apply_local_thresholding(image: &Array<f32>) -> Array<f32> {

    let height = image.shape[0] as isize;
    let width = image.shape[1] as isize;

    let image_ref = Image1cRef::new(&image.data[..], height as usize, width as usize);

    // TODO: Just init to a zeroed one.
    let mut out = image.clone();
    let mut out_ref = Image1cRef::new(&mut out.data[..], height as usize, width as usize);

    for y in 0..height {
        for x in 0..width {

            let mut min_intensity = 1000.0f32;
            let mut max_intensity = 0.0f32;

            for y_step in -LOCAL_THRESHOLDING_RADIUS..(LOCAL_THRESHOLDING_RADIUS + 1) {
                let y_i = y + y_step;
                if y_i < 0 || y_i >= height {
                    continue;
                }

                for x_step in -LOCAL_THRESHOLDING_RADIUS..(LOCAL_THRESHOLDING_RADIUS + 1) {
                    let x_i = x + x_step;
                    if x_i < 0 || x_i >= width {
                        continue;
                    }

                    let v = image_ref.get(y_i as usize, x_i as usize);
                    min_intensity = min_intensity.min(v);
                    max_intensity = max_intensity.max(v);
                }
            }

            let mut threshold = min_intensity + LOCAL_THRESHOLDING_RELATIVE_THRESHOLD * (max_intensity - min_intensity);
            threshold = threshold.max(LOCAL_THRESHOLDING_ABSOLUTE_THRESHOLD);

            let v = if image_ref.get(y as usize, x as usize) >= threshold { 255.0 } else { 0.0 };
            out_ref.set(y as usize, x as usize, v);
        }
    }

    out
}

fn apply_condional_dilation(image: &Array<f32>) -> Array<f32> {
    let height = image.shape[0];
    let width = image.shape[1];

    let image_ref = Image1cRef::new(&image.data[..], height, width);

    // TODO: Just init to a zeroed one.
    let mut out = image.clone();

    for y in 0..height {
        for x in 0..width {

            let v = image_ref.get(y, x);

            // Already 'true' so no point in testing neighbors.
            if v != 0.0 {
                continue;
            }

            let mut num_true_neighbors = 0;

            image_ref.visit_neighbors(y, x, |_, _, v| {
                if v != 0.0 {
                    num_true_neighbors += 1;
                }
            });

            if num_true_neighbors < DILATION_MIN_NEIGHBORS {
                continue;
            }
            
            out[&[y as usize, x as usize][..]] = 255.0;
        }
    }

    out

}

/// Calculates the squared distance to background (0 pixels)
///
/// TODO: Use integers for this.
///
/// TODO: Flip row and column order since that would make this much faster since images is typically wider than taller.
///
/// See https://arxiv.org/abs/2106.03503
fn calculate_distance_transform(image: &Array<f32>) -> Array<u32> {
    let height = image.shape[0] as isize;
    let width = image.shape[1] as isize;

    let mut horizontal_dists = Array {
        shape: image.shape.clone(),
        data: vec![0u32; image.data.len()]
    };

    // Initial distances
    for i in 0..horizontal_dists.size() {
        if image[i] == 0.0 {
            horizontal_dists[i] = 0;
        } else {
            // infinity.
            horizontal_dists[i] = (height * height + width * width) as u32; 
        }
    }

    // Compute vertical distances (column-wise)
    let mut horizontal_dists_ref = Image1cRef::new(&mut horizontal_dists.data[..], height as usize, width as usize);
    for x in 0..width {
        let mut dist_step = 1;

        // Propagate distances down.
        for y in 1..height {
            let prev = horizontal_dists_ref.get((y - 1) as usize, (x as usize));
            let cur = horizontal_dists_ref.get(y as usize, x as usize);

            if cur > prev + dist_step {
                horizontal_dists_ref.set(y as usize, x as usize, prev + dist_step);
                dist_step += 2;
            } else {
                dist_step = 1;
            }
        }

        // Propagate distances upward
        dist_step = 1;
        for y in (0..(height - 1)).rev() {
            let next = horizontal_dists_ref.get((y + 1) as usize, (x  as usize));
            let cur = horizontal_dists_ref.get(y as usize, x as usize);

            if cur > next + dist_step {
                horizontal_dists_ref.set(y as usize, x as usize, next + dist_step);
                dist_step += 2;
            } else {
                dist_step = 1;
            }
        }
    }
    
    let mut out = Array {
        shape: image.shape.clone(),
        data: vec![0u32; image.data.len()]
    };

    // Compute horizontal distances (row-wise)
    for y in 0..height {

        for x in 0..width {
            let mut min_dist = horizontal_dists_ref.get(y as usize, x as usize);

            for x_i in 0..width {
                let delta = (x_i as isize) - (x as isize);

                let dist = horizontal_dists_ref.get(y as usize, x_i as usize) + ((delta * delta) as u32);
                if dist < min_dist {
                    min_dist = dist;
                }
            }

            out[&[y, x][..]] = min_dist;
        }
    }

    out
}

// Finds the center line by selecting all pixels which are the peaks of the background_distance function relative to its
// neighbors in at least one direction.
fn find_centerline(image: &Array<f32>, background_distances: &Array<u32>) -> Array<f32> {
    let height = image.shape[0] as isize;
    let width = image.shape[1] as isize;

    let mut out = Array {
        shape: image.shape.clone(),
        data: vec![0.0; image.data.len()],
    };

    // With (0, 0) being the center pixel, these are opposite pairs of pixels around
    // the center pixel.
    let pairs = [
        ((-1,  0), ( 1,  0)), 
        (( 0, -1), ( 0,  1)), 
        ((-1, -1), ( 1,  1)), 
        ((-1,  1), ( 1, -1)), 
    ];

    for y in 0..height {
        for x in 0..width {
            let cur_dist = background_distances[&[y as usize, x as usize][..]];

            // Skip background
            if cur_dist == 0 {
                continue;
            }

            let mut is_ridge = false;

            for ((y1, x1), (y2, x2)) in pairs.iter() {
                let ny1 = y + y1;
                let nx1 = x + x1;
                let ny2 = y + y2;
                let nx2 = x + x2;

                let d1 = if ny1 >= 0 && nx1 >= 0 && ny1 < height && nx1 < width {
                    background_distances[&[ny1 as usize, nx1 as usize][..]]
                } else { 0 };

                let d2 = if ny2 >= 0 && nx2 >= 0 && ny2 < height && nx2 < width {
                    background_distances[&[ny2 as usize, nx2 as usize][..]]
                } else { 0 };


                // NOTE: The goal of using '>' and '>=' is to prevent 2 wide lines
                // on flat areas.
                if cur_dist > d1 && cur_dist >= d2 {
                    is_ridge = true;
                    break;
                }
            }

            if is_ridge {
                out[&[y as usize, x as usize][..]] = 255.0;
            }
        }
    }

    out
}




// Removes all pixels with <= 1 neighbors
// (and recursively follow the path created if this introduces new pixels with just one neighbor.)
fn prune_dead_ends(input: &mut Array<f32>) {
    let height = input.shape[0] as isize;
    let width = input.shape[1] as isize;

    for y_base in 0..height {
        for x_base in 0..width {

            let v = input[&[y_base as usize, x_base as usize][..]];
            
            // Skip background pixels.
            if v == 0.0 {
                continue;
            }

            let mut next_pixel = Some((y_base, x_base));

            while let Some((y, x)) = next_pixel.take() {
                let mut num_true_neighbors = 0;

                // Searching all 8 neighbors.
                for y_step in -1..2 {
                    let y_i = y + y_step;
                    if y_i < 0 || y_i >= height {
                        continue;
                    }

                    for x_step in -1..2 {
                        let x_i = x + x_step;
                        if x_i < 0 || x_i >= width {
                            continue;
                        }

                        // Don't count ourselves.
                        if x_i == x && y_i == y {
                            continue;
                        }

                        let v = input[&[y_i as usize, x_i as usize][..]];
                        if v != 0.0 {
                            next_pixel = Some((y_i, x_i));
                            num_true_neighbors += 1;
                        }
                    }
                }

                if num_true_neighbors <= 1 {
                    input[&[y as usize, x as usize][..]] = 0.0;
                } else {
                    break;
                }
            }
        }
    }
}

// Combines any neighboring pixels where the set of neighbors connected to one pixel
// are a superset of one of its neighbors.
//
// TODO: This currently has a chance of making lines more jagged if we choose to merge
// in the direction of a line splinter rather than into the line.
//
// TODO: If a pixel is black and all neighbors are white and have no other neighbors, maybe
// make the current pixel white (or just make everything in that area black to expedite things)
fn merge_redundant_pixels(input: &mut Array<f32>) {
    let height = input.shape[0] as isize;
    let width = input.shape[1] as isize;

    for y_base in 0..height {
        for x_base in 0..width {

            let v = input[&[y_base as usize, x_base as usize][..]];
            
            // Skip background pixels.
            if v == 0.0 {
                continue;
            }

            // Searching all 8 neighbors.
            for y_step in -1..2 {
                let y_i = y_base + y_step;
                if y_i < 0 || y_i >= height {
                    continue;
                }

                for x_step in -1..2 {
                    let x_i = x_base + x_step;
                    if x_i < 0 || x_i >= width {
                        continue;
                    }

                    // Don't count ourselves.
                    if x_i == x_base && y_i == y_base {
                        continue;
                    }

                    maybe_merge_with_neighbor(x_base, y_base, x_i, y_i, input);
                }
            }
        }
    }
}

fn maybe_merge_with_neighbor(
    x_base: isize, y_base: isize,
    x_neighbor: isize, y_neighbor: isize,
    input: &mut Array<f32>
) {
    let height = input.shape[0] as isize;
    let width = input.shape[1] as isize;

    let v = input[&[y_neighbor as usize, x_neighbor as usize][..]];
    
    // Skip background pixels.
    if v == 0.0 {
        return;
    }

    // Searching all 8 neighbors.
    for y_step in -1..2 {
        let y_i = y_neighbor + y_step;
        if y_i < 0 || y_i >= height {
            continue;
        }

        for x_step in -1..2 {
            let x_i = x_neighbor + x_step;
            if x_i < 0 || x_i >= width {
                continue;
            }

            // Don't count ourselves.
            if (x_i == x_neighbor && y_i == y_neighbor) || (x_i == x_base && y_i == y_base) {
                continue;
            }

            let v = input[&[y_i as usize, x_i as usize][..]];
            if v != 0.0 {

                if (x_i - x_base).abs() > 1 || (y_i - y_base).abs() > 1 {
                    return;
                }
            }
        }
    }

    input[&[y_neighbor as usize, x_neighbor as usize][..]] = 0.0;
}

// This finds all points with >= 3 neighbors
// Returns a list of (y,x) coordinates
fn find_saddle_points(input: &Array<f32>) -> Vec<(usize, usize)> {
    let height = input.shape[0] as isize;
    let width = input.shape[1] as isize;

    let mut out = vec![];

    for y_base in 0..height {
        for x_base in 0..width {

            let v = input[&[y_base as usize, x_base as usize][..]];
            
            // Skip background pixels.
            if v == 0.0 {
                continue;
            }

            let mut num_true_neighbors = 0;

            // Searching all 8 neighbors.
            for y_step in -1..2 {
                let y_i = y_base + y_step;
                if y_i < 0 || y_i >= height {
                    continue;
                }

                for x_step in -1..2 {
                    let x_i = x_base + x_step;
                    if x_i < 0 || x_i >= width {
                        continue;
                    }

                    // Don't count ourselves.
                    if x_i == x_base && y_i == y_base {
                        continue;
                    }

                    let v = input[&[y_i as usize, x_i as usize][..]];
                    if v != 0.0 {
                        num_true_neighbors += 1;
                    }
                }
            }

            if num_true_neighbors >= 3 {
                out.push((y_base as usize, x_base as usize)); 
            }
        }
    }

    out
}

#[derive(Clone)]
struct SaddlePointCluster {
    points: Vec<(usize, usize)>
}

impl SaddlePointCluster {

    // TODO: Make this use mid-pixel centers?
    fn average_point(&self) -> Vector2f {

        let mut x = 0;
        let mut y = 0;

        for (y_i, x_i) in self.points.iter().cloned() {
            x += x_i;
            y += y_i;
        }

        let n = self.points.len() as f32;

        vec2f(
            (x as f32) / n,
            (y as f32) / n,
        )
    }
}

fn merge_saddle_points(mut raw_points: Vec<(usize, usize)>, radius: f32) -> Vec<SaddlePointCluster> {

    let radius_squared = radius * radius;

    let mut out = vec![];

    while let Some(mut pt) = raw_points.pop() {

        let mut points = vec![];
        points.push(pt);

        let mut i = 0;
        while i < points.len() {

            let mut j = 0;
            while j < raw_points.len() {
                let pt = &points[i];
                let pt2 = raw_points[j].clone();
                let dist = squared((pt.0 as f32) - (pt2.0 as f32)) +
                    squared((pt.1 as f32) - (pt2.1 as f32));

                if dist <= radius_squared {
                    points.push(raw_points.swap_remove(j));
                    continue;
                }

                j += 1;
            }

            i += 1;
        }

        out.push(SaddlePointCluster {
            points
        });
    }

    out
}


fn find_adjacent_clusters(input: &Array<f32>, saddle_clusters: &[SaddlePointCluster]) -> Vec<(usize, usize)> {
    let input_ref = Image1cRef::new(&input.data[..], input.shape[0], input.shape[1]);

    // // Map from saddle point coordinates to cluster index.
    let mut saddle_cluster_index = HashMap::<(usize, usize), usize>::default();
    for (i, c) in saddle_clusters.iter().enumerate() {
        for pt in c.points.iter().cloned() {
            saddle_cluster_index.insert(pt, i);
        }
    }

    let mut out = vec![];

    for (i, c) in saddle_clusters.iter().enumerate() {
        let mut visited = HashSet::<(usize, usize), FastHasherBuilder>::default();
        let mut queue = vec![];
        for pt in c.points.iter().cloned() {
            visited.insert(pt);
            queue.push(pt);
        }

        while let Some(pt) = queue.pop() {
            input_ref.visit_neighbors(pt.0, pt.1, |y, x, v| {
                if v == 0.0 {
                    return;
                }

                if let Some(other_cluster) = saddle_cluster_index.get(&(y, x)).cloned() {
                    if i != other_cluster {
                        out.push((i, other_cluster));
                    }

                    return;
                }

                if visited.insert((y, x)) {
                    queue.push((y, x));
                }
            });
        }
    }

    out
}

#[derive(Default)]
struct SaddlePointsComponent {
    clusters: HashSet<usize, FastHasherBuilder>
}

fn connect_cluster_components(
    saddle_clusters: &[SaddlePointCluster],
    edges: &[(usize, usize)]
) -> Vec<SaddlePointsComponent> {

    let mut sets = crate::connected_components::DisjointSets::default();
    for i in 0..saddle_clusters.len() {
        let j = sets.new_set();
        assert_eq!(i, j);
    }

    for (i, j) in edges.iter().cloned() {
        sets.union(i, j);
    }

    // TODO: Change to use flatten_and_relabel and then we probably don't need the retain.
    sets.flatten();

    let mut out = vec![];
    for i in 0..saddle_clusters.len() {
        out.push(SaddlePointsComponent::default());
    }

    for i in 0..saddle_clusters.len() {
        out[sets.find_root(i)].clusters.insert(i);
    }

    out.retain(|v| v.clusters.len() > 0);

    out
}

#[derive(Debug)]
struct CheckerboardGeometry {
    width: usize,
    height: usize,
    // row by row list of which saddle point clusters correct to each part of the grid.
    clusters: Vec<usize>
}

impl CheckerboardGeometry {
    fn transpose(&self) -> Self {
        let mut clusters = vec![];
        for j in 0..self.width {
            for i in 0..self.height {
                clusters.push(self.clusters[
                    i * self.width + j
                ]);
            }
        }

        Self {
            width: self.height,
            height: self.width,
            clusters
        }
    }

    fn flip_x(&self) -> Self {
        let mut clusters = vec![];
        for i in 0..self.height {
            for j in (0..self.width).rev() {
                clusters.push(self.clusters[
                    i * self.width + j
                ]);
            }
        }

        Self {
            width: self.width,
            height: self.height,
            clusters
        }

    }

}

fn check_checkerboard_geometry(
    saddle_clusters: &[SaddlePointCluster], saddle_edges: &[(usize, usize)],
    component: &SaddlePointsComponent
) -> Option<CheckerboardGeometry> {

    let mut edge_graph = HashMap::<usize, HashSet<usize, FastHasherBuilder>, FastHasherBuilder>::default();
    for (i, j) in saddle_edges.iter().cloned() {
        edge_graph.entry(i).or_default().insert(j);
        edge_graph.entry(j).or_default().insert(i);
    }

    // Find one of the outer corners of the checkerbaord
    let corner_idx = match component.clusters.iter().find(|idx| {
        let edges = match edge_graph.get(idx) {
            Some(v) => v,
            None => return false
        };

        edges.len() == 2
    }) {
        Some(v) => *v,
        None => return None
    };
    
    let mut visited = HashSet::<usize, FastHasherBuilder>::default();

    let mut out: Vec<usize> = vec![];

    out.push(corner_idx);
    visited.insert(corner_idx);

    // Follow one of the edges to find the 'width' of the grid.
    let mut width = 0;
    loop {
        let last_corner = out.last().unwrap();
        let last_corner_edges = edge_graph.get(&last_corner).unwrap();

        let mut next_corner = None;
        let mut next_corner_count = 1000;

        for other_idx in last_corner_edges.iter().cloned() {
            if visited.contains(&other_idx) {
                continue;
            }

            let other_edges = edge_graph.get(&other_idx).unwrap();
            if other_edges.len() < next_corner_count {
                next_corner = Some(other_idx);
                next_corner_count = other_edges.len();
            }
        }

        let next_corner = match next_corner {
            Some(v) => v,
            None => return None
        };

        out.push(next_corner);
        visited.insert(next_corner);

        if next_corner_count == 2 {
            // Hit the other corner.
            width = out.len();
            break;
        } else if next_corner_count == 3 {
            // Still following the edge.
        } else {
            // Failed to follow a clean grid edge.
            return None;
        }
    }

    // Add connecting rows.
    let mut in_final_row = false;
    let mut hit_final_cell = false;
    loop {
        let above_corner = out[out.len() - width];
        let above_corner_edges = edge_graph.get(&above_corner).unwrap();

        let corner = match above_corner_edges.iter().find(|other_idx| {
            !visited.contains(other_idx)
        }) {
            Some(v) => *v,
            None => return None
        };

        // TODO: eventually check the full list of edges from the current corner for
        // expected 4 way connections.
        let corner_edges = edge_graph.get(&corner).unwrap();

        let x = out.len() % width;
        // Check for left connectivity.
        if x > 0 {
            if !corner_edges.contains(&out[out.len() - 1]) {
                return None;
            }
        }

        out.push(corner);
        visited.insert(corner);

        if corner_edges.len() == 2 {
            if in_final_row {
                hit_final_cell = true;
                break;
            }

            in_final_row = true;
        }
    }


    if !hit_final_cell {
        return None;
    }

    if out.len() % width != 0 {
        return None;
    }

    // Just in case we missed checking some edges.
    if out.len() != component.clusters.len() {
        return None;
    }

    let height = out.len() / width;

    Some(CheckerboardGeometry {
        width,
        height,
        clusters: out
    })
}


