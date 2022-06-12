use core::ops::AddAssign;

pub trait TreeAttribute = Default + AddAssign + Copy;

#[derive(Clone, Copy, Default, Debug)]
pub struct EmptyAttribute {
    _hidden: (),
}

impl AddAssign for EmptyAttribute {
    fn add_assign(&mut self, rhs: Self) {
        //
    }
}
