
// Also need to be able to select fixed value for any dimension and take all indices matching that
// - Basically generalize selecting a channel

// Support taking absolute value of an entire Array

const PI: f32 = 3.14159;

enum Direction {
	Horizonal = 0,
	Vertical = 1,
	PosDiagonal = 2, // 45 degrees
	NegDiagonal = 3 // -45 degrees
}

// TODO: For canny, have a detector struct that caches the arrays for doing many items of the same size

// TODO: Move into a separate folder for this stuff
struct CannyEdgeDetector {

}

impl CannyEdgeDetector {
	pub fn detect(image: &Image<u8>) -> Image<u8> {
		let mut gauss = Array::<f32>::from_slice(
			&[2.0, 4.0, 5.0, 4.0, 2.0,
				4.0, 9.0, 12.0, 9.0, 4.0,
				5.0, 12.0, 15.0, 12.0, 5.0,
				4.0, 9.0, 12.0, 9.0, 4.0,
				2.0, 4.0, 5.0, 4.0, 2.0]).reshape(&[5,5]);
		gauss = gauss * (1.0 / 159.0);

		let sobel_y = Array::<f32>::from_slice(
			&[-1.0, 0.0, 1.0,
				-2.0, 0.0, 2.0,
				-1.0, 0.0, 1.0])
			.reshape(&[3,3]);

		// TODO: Just transpose the sobel_x
		let sobel_x = Array::<f32>::from_slice(
			&[-1.0, -2.0, -1.0,
				0.0, 0.0, 0.0,
				1.0, 2.0, 1.0])
			.reshape(&[3,3]);

		// i16?
		// Convert to grayscale and drop the color dimension.
		// TODO: grayscale should be a no-op if already in one channel
		let arr = image.to_grayscale().array().reshape(&[image.height(), image.width()]).cast::<f32>();

		let blurred = arr.cross_correlate(&gauss, KernelEdgeMode::Mirror);

		let gx = blurred.cross_correlate(&sobel_x, KernelEdgeMode::Mirror);
		let gy = blurred.cross_correlate(&sobel_y, KernelEdgeMode::Mirror);

		let g_dir = gx.zip(&gy, |x, y| {
			let mut d = y.atan2(x); // [-pi, pi]
			d *= 2.0/PI; // [-2, 2]

			let dq = d.round() as i8;
			if dq == 0 {
				Direction::Horizonal
			} else if dq == 1 {
				Direction::PosDiagonal
			} else if dq == 2 || dq == -2 {
				Direction::Vertical
			} else if dq == -1 {
				Direction::NegDiagonal
			} else {
				panic!("Should never happen");
			}
		});

		let g_mag = gx.zip(&gy, |x, y| {
			(x.powf(2.0) + y.powf(2.0)).sqrt() as u8
		});

		// Perform non-maximum suppression.
		let mut g_mag_suppresed = {
			let mut g_mag_iter = g_mag.iter();
			let mut g_dir_iter = g_dir.data.iter();

			let mut data = Vec::new();

			while let Some(dir) = g_dir_iter.next() {
				{
					// TODO: If already zero, just skip ahead

					let pos = g_mag_iter.pos().unwrap();
					let (del_a, del_b): (Array<isize>, Array<isize>) = match dir {
						Direction::Horizonal => (vec![0, 1].into(), vec![0, -1].into()),
						Direction::Vertical => (vec![1, 0].into(), vec![-1, 0].into()),
						Direction::PosDiagonal => (vec![1, 1].into(), vec![-1, -1].into()),
						Direction::NegDiagonal => (vec![-1, 1].into(), vec![1, -1].into())
					};

					let pos_a = pos.clone() + &del_a;
					let pos_b = pos.clone() + &del_b;

					let mut max = 0;
					if g_mag.contains_pos(&pos_a.data[..]) {
						max = g_mag[&pos_a.data[..]];
					}
					if g_mag.contains_pos(&pos_b.data[..]) {
						max = std::cmp::max(max, g_mag[&pos_b.data[..]]);
					}

					let mut v = g_mag[&pos.data[..]];
					if v < max {
						v = 0;
					}

					data.push(v)
				}
				g_mag_iter.step();
			}

			Array::from(data).reshape(&g_mag.shape)
		};

		// TODO: https://en.wikipedia.org/wiki/Otsu%27s_method for determining best thresholds.
		let threshold_low = 50;
		let threshold_high = 100;

		assert!(threshold_high > threshold_low);

		// Generate sets for all pixels > threshold_low
		let mut sets = core::algorithms::DisjointSets::new(g_mag_suppresed.data.len());
		let img_width = g_mag.shape[1];
		for i in 0..g_mag_suppresed.data.len() {
			let val = g_mag_suppresed.data[i];

			if val < threshold_low {
				continue;
			}

			// Previous neighbors in 8-connectivity
			// These are ordered to go from lowest to highest index
			let negative_offsets = vec![
				img_width + 1,
				img_width, // up
				img_width - 1, // up-right
				1 // left
			];

			for off in negative_offsets.iter() {
				if i >= *off && g_mag_suppresed[i - off] > threshold_low {
					sets.union_sets(i, i - off);
				}
			}
		}

		// Find ids of all sets containing a strong pixel.
		// The value in the hashmap will be the number of *strong* pixels in the set.
		let mut strong_sets = std::collections::HashMap::<usize, usize>::new();
		for i in 0..g_mag_suppresed.data.len() {
			let val = g_mag_suppresed.data[i];
			if val > threshold_high {
				let set_id = sets.find_set(i);
				let count = strong_sets.get(&set_id).cloned().unwrap_or(0) + 1;
				strong_sets.insert(set_id, count);
			}
		}

		let min_num_strong_pixels = 2;

		// Finally accept pixels in a good set
		for i in 0..g_mag_suppresed.data.len() {
			let set_id = sets.find_set(i);
			g_mag_suppresed[i] =
				if strong_sets.get(&set_id).cloned().unwrap_or(0) > min_num_strong_pixels {
					255
				} else {
					0
				};
		}

		Image::new(g_mag_suppresed)
	}
}
