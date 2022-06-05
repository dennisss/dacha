use core::ops::{Deref, DerefMut};

use crate::canvas::base::CanvasBase;

pub trait Canvas: Deref<Target = CanvasBase> {
    //
}
