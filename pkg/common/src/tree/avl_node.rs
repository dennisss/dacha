use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// Single node of an AVLTree.
///
/// This is a separate module with all private fields to ensure that the height
/// field is always consistently updated after mutations.
#[derive(Clone, Debug)]
pub struct AVLNode<T> {
    value: T,

    // TODO: Optimize to only store a -2 to 2 balance factor value.
    height: isize,
    left: Option<Box<AVLNode<T>>>,
    right: Option<Box<AVLNode<T>>>,
}

impl<T: PartialEq> PartialEq for AVLNode<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.left == other.left && self.right == other.right
    }
}

impl<T> AVLNode<T> {
    pub fn new(value: T, left: Option<Box<AVLNode<T>>>, right: Option<Box<AVLNode<T>>>) -> Self {
        Self {
            value,
            height: -1,
            left,
            right,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn value_right_mut(&mut self) -> (&mut T, &mut Option<Box<AVLNode<T>>>) {
        self.height = -1;
        (&mut self.value, &mut self.right)
    }

    pub fn into_value(self) -> T {
        self.value
    }

    pub fn left(&self) -> Option<&Box<AVLNode<T>>> {
        self.left.as_ref()
    }

    pub fn left_mut(&mut self) -> &mut Option<Box<AVLNode<T>>> {
        self.height = -1;
        &mut self.left
    }

    pub fn set_left(&mut self, node: Option<Box<AVLNode<T>>>) {
        self.height = -1;
        self.left = node;
    }

    pub fn take_left(&mut self) -> Option<Box<AVLNode<T>>> {
        self.height = -1;
        self.left.take()
    }

    pub fn right(&self) -> Option<&Box<AVLNode<T>>> {
        self.right.as_ref()
    }

    pub fn right_mut(&mut self) -> &mut Option<Box<AVLNode<T>>> {
        self.height = -1;
        &mut self.right
    }

    pub fn set_right(&mut self, node: Option<Box<AVLNode<T>>>) {
        self.height = -1;
        self.right = node;
    }

    pub fn take_right(&mut self) -> Option<Box<AVLNode<T>>> {
        self.height = -1;
        self.right.take()
    }

    pub fn height(&mut self) -> isize {
        if self.height == -1 {
            self.recalculate_height();
        }

        self.height
    }

    fn recalculate_height(&mut self) {
        let left_height = self.left.as_mut().map(|n| n.height()).unwrap_or(-1);
        let right_height = self.right.as_mut().map(|n| n.height()).unwrap_or(-1);
        self.height = 1 + core::cmp::max(left_height, right_height);
    }

    pub fn balance_factor(&mut self) -> isize {
        let left_height = self.left.as_mut().map(|n| n.height()).unwrap_or(-1);
        let right_height = self.right.as_mut().map(|n| n.height()).unwrap_or(-1);
        right_height - left_height
    }
}
