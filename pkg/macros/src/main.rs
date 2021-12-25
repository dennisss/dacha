#[macro_use]
extern crate macros;

// struct Apples<T> {
// 	data: T
// }
// blanket! {
// 	impl<T: std::ops::Add<usize>> Apples<T> {}

// 	impl Apples {

// 	}

// 	impl Apples {
// 		fn eat(&self) -> T {
// 			self + 2
// 		}
// 	}
// }

// fn hello() -> usize {
//     i
// }

range_param!(i = 1..10, {
    impl<T: ConstDefault> ConstDefault for [T; i] {
        const DEFAULT: Self = [T::DEFAULT; i];
    }
});

fn main() {
    println!("Hello");
}
