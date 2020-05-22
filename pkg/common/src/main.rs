extern crate common;

use common::args::Arg;

fn main() {
    let my_bool = Arg::<bool>::required("enabled");
    let my_string = Arg::<String>::optional("path", "/dev/null");
    common::args::init(&[&my_bool, &my_string]).unwrap();

    println!("{} : {}", my_bool.borrow(), my_string.borrow());
}
