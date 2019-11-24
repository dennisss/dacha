
extern crate math;

use math::matrix::*;


fn main() {
	let mat = Matrix3f::from_slice(&[
		1., 2., 3.,
		4., 5., 6.,
		7., 8., 9.
	]);
	let mat2 = Matrix3f::from_slice(&[
		1., 2., 3.,
		4., 5., 6.,
		7., 8., 9.
	]);

	println!("{:?}", &mat * &mat2);
}