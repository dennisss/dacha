
use std::cmp::Ordering;

/// TODO: Generalize output as an iterator or a stream.
pub fn merge_by<T, F>(a_list: Vec<T>, b_list: Vec<T>, mut f: F) -> Vec<T>
	where F: FnMut(&T, &T) -> Ordering {
	let mut a_iter = a_list.into_iter();
	let mut b_iter = b_list.into_iter();

	let mut out = vec![];

	let mut a_val = None;
	let mut b_val = None;
	loop {
		if a_val.is_none() {
			a_val = a_iter.next();
		}
		if b_val.is_none() {
			b_val = b_iter.next();
		}

		if a_val.is_none() || b_val.is_none() {
			break;
		}

		let a = a_val.take().unwrap();
		let b = b_val.take().unwrap();

		match f(&a, &b) {
			Ordering::Equal => {
				out.push(a);
				// Both a_val, b_val remain None
			},
			Ordering::Less => {
				out.push(a);
				b_val = Some(b);
			},
			Ordering::Greater => {
				out.push(b);
				a_val = Some(a);
			}
		}
	}

	// Push remainder

	if let Some(a) = a_val {
		out.push(a);
	}
	if let Some(b) = b_val {
		out.push(b);
	}

	for v in a_iter.chain(b_iter) {
		out.push(v);
	}

	out
}
