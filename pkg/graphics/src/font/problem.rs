trait Number {
    fn number(&self) -> usize;
}

struct One {}
impl Number for One {
    fn number(&self) -> usize {
        1
    }
}

trait NumberExt: Number {
    fn double_number(&self) -> usize {
        double_number_helper(self)
    }
}

impl<T: Number + ?Sized> NumberExt for T {}

// Existing helper function (prefer not to change this code).
fn double_number_helper(value: &dyn Number) -> usize {
    value.number() * 2
}

fn test(value: &One) {
    println!("Double my number is: {}", value.double_number());
}

fn test2<T: Number>(value: &T) {
    println!("Double my number is: {}", value.double_number());
}

fn test3(value: &dyn Number) {
    println!("Double my number is: {}", value.double_number());
}

fn main() {
    test(&One {});
    test2(&One {});
    test3(&One {});
}
