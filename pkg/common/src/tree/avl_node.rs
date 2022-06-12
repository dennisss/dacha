use alloc::boxed::Box;
use core::cmp::Ordering;

use crate::tree::attribute::TreeAttribute;

/// Single node of an AVLTree.
///
/// This is a separate module with all private fields to ensure that the height
/// field is always consistently updated after mutations.
#[derive(Clone, Debug)]
pub struct AVLNode<T, A> {
    value: T,

    /// Attribute associated with this node's value.
    value_attribute: A,

    /// Height of the subtree rooted at this node.
    ///
    /// A node with no children has a height of 0.
    ///
    /// TODO: Optimize to
    /// only store a -2 to 2 balance factor value.
    height: isize,

    /// Sum of all attributes in this subtree.
    /// This is maintained as a cached value which is recomputed when the tree
    /// changes.
    subtree_attributes: A,

    left: Option<Box<AVLNode<T, A>>>,
    right: Option<Box<AVLNode<T, A>>>,
}

impl<T: PartialEq, A> PartialEq for AVLNode<T, A> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.left == other.left && self.right == other.right
    }
}

impl<T, A: TreeAttribute> AVLNode<T, A> {
    pub fn new(
        value: T,
        value_attribute: A,
        left: Option<Box<AVLNode<T, A>>>,
        right: Option<Box<AVLNode<T, A>>>,
    ) -> Self {
        let mut inst = Self {
            value,
            value_attribute,
            height: 0,                        // Calculated below.
            subtree_attributes: A::default(), // Calculated below.
            left,
            right,
        };

        inst.recalculate();
        inst
    }

    pub fn get_mut(&mut self) -> AVLNodeGuard<T, A> {
        AVLNodeGuard { node: self }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_attribute(&self) -> A {
        self.value_attribute
    }

    /// NOTE: We allow assessing this without a guard as no tree attributes are
    /// derived from this.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }

    pub fn left(&self) -> Option<&Box<AVLNode<T, A>>> {
        self.left.as_ref()
    }

    pub fn set_left(&mut self, node: Option<Box<AVLNode<T, A>>>) {
        self.left = node;
        self.recalculate();
    }

    pub fn take_left(&mut self) -> Option<Box<AVLNode<T, A>>> {
        let v = self.left.take();
        self.recalculate();
        v
    }

    pub fn right(&self) -> Option<&Box<AVLNode<T, A>>> {
        self.right.as_ref()
    }

    /// Checks whether or not 'other' is the right child of the current node.
    pub fn right_child_is(&self, other: &AVLNode<T, A>) -> bool {
        self.right
            .as_ref()
            .map(|node| core::ptr::eq(node.as_ref(), other))
            .unwrap_or(false)
    }

    pub fn set_right(&mut self, node: Option<Box<AVLNode<T, A>>>) {
        self.right = node;
        self.recalculate();
    }

    pub fn take_right(&mut self) -> Option<Box<AVLNode<T, A>>> {
        let v = self.right.take();
        self.recalculate();
        v
    }

    pub fn height(&self) -> isize {
        self.height
    }

    pub fn subtree_attributes(&self) -> A {
        self.subtree_attributes
    }

    fn recalculate(&mut self) {
        self.recalculate_height();
        self.recalculate_subtree_attributes();
    }

    fn recalculate_height(&mut self) {
        let left_height = self.left.as_mut().map(|n| n.height()).unwrap_or(-1);
        let right_height = self.right.as_mut().map(|n| n.height()).unwrap_or(-1);
        self.height = 1 + core::cmp::max(left_height, right_height);
    }

    fn recalculate_subtree_attributes(&mut self) {
        let mut subtree_attributes = self.value_attribute;
        if let Some(left) = self.left.as_mut() {
            subtree_attributes += left.subtree_attributes();
        }
        if let Some(right) = self.right.as_mut() {
            subtree_attributes += right.subtree_attributes();
        }

        self.subtree_attributes = subtree_attributes;
    }

    pub fn balance_factor(&self) -> isize {
        let left_height = self.left.as_ref().map(|n| n.height()).unwrap_or(-1);
        let right_height = self.right.as_ref().map(|n| n.height()).unwrap_or(-1);
        right_height - left_height
    }
}

/// Safe guard to get a mutator reference to data inside a node. Once
/// this is dropped, tree attributes are recalculated.
pub struct AVLNodeGuard<'a, T, A: TreeAttribute> {
    node: &'a mut AVLNode<T, A>,
}

impl<'a, T, A: TreeAttribute> Drop for AVLNodeGuard<'a, T, A> {
    fn drop(&mut self) {
        self.node.recalculate();
    }
}

impl<'a, T, A: TreeAttribute> AVLNodeGuard<'a, T, A> {
    pub fn left_mut(&mut self) -> &mut Option<Box<AVLNode<T, A>>> {
        &mut self.node.left
    }

    pub fn right_mut(&mut self) -> &mut Option<Box<AVLNode<T, A>>> {
        &mut self.node.right
    }

    pub fn value_right_mut(&mut self) -> (&mut T, &mut A, &mut Option<Box<AVLNode<T, A>>>) {
        (
            &mut self.node.value,
            &mut self.node.value_attribute,
            &mut self.node.right,
        )
    }

    pub fn value_attribute_mut(&mut self) -> &mut A {
        &mut self.node.value_attribute
    }
}
