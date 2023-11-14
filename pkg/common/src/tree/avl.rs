use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::tree::attribute::{EmptyAttribute, TreeAttribute};
use crate::tree::avl_node::AVLNode;
use crate::tree::comparator::*;

/// Self-balancing binary search tree where the left and right sub-trees of any
/// node always have an absolute height difference of <= 1.
///
/// Optimization TODOs:
/// - For simplicity, nodes store the height of each subtree rather than a [-2,
///   2] balance factor.
/// - Insert/remove are implemented as recursive functions which attempt to
///   repair each node on the way back up the call stack. We should skip
///   balancing a node if no height changes occured in its children.
#[derive(Clone, Debug)]
pub struct AVLTree<T, A = EmptyAttribute, C = OrdComparator> {
    root: Option<Box<AVLNode<T, A>>>,
    comparator: C,
}

impl<T: Ord> AVLTree<T, EmptyAttribute, OrdComparator> {
    pub fn default() -> Self {
        Self::new(OrdComparator::default())
    }
}

impl<T, A: TreeAttribute, C: Comparator<T, T>> AVLTree<T, A, C> {
    pub fn new(comparator: C) -> Self {
        Self {
            root: None,
            comparator,
        }
    }

    /// Changes the comparator used for future tree operations.
    ///
    /// The new comparator should perform an equivalent ordering of all elements
    /// in the tree as the old comparator.
    ///
    /// NOTE: This is a dangerous function as it doesn't re-sort the contents of
    /// the tree and assumes that the user knows what they are doing.
    pub fn change_comparator(&mut self, comparator: C) {
        self.comparator = comparator;
    }

    /// Helper for allocating a new buffer for storing a chain of node pointers
    /// from the root to a leaf.
    ///
    /// Because we know the height of the tree precisely, we can allocate a
    /// buffer right away of the current size which will never be re-allocated.
    fn allocate_path_buf<'a>(&'a self) -> Vec<&'a AVLNode<T, A>> {
        let mut path = vec![];
        if let Some(node) = self.root.as_ref() {
            path.reserve_exact((1 + node.height()) as usize);
        }
        path
    }

    /// Lookups up a value in the tree which equals the query.
    ///
    /// If such a value exists, then an iterator will be returned pointing to
    /// that value will be returned. Else, None will be returned.
    pub fn find<'a, Q>(&'a self, query: &Q) -> Option<Iter<'a, T, A>>
    where
        C: Comparator<T, Q>,
    {
        let mut path = self.allocate_path_buf();

        let mut next_pointer = self.root.as_ref();
        while let Some(node) = next_pointer {
            path.push(node.as_ref());

            match self.comparator.compare(node.value(), query) {
                Ordering::Equal => {
                    return Some(Iter {
                        root: self.root.as_ref().map(|n| n.as_ref()),
                        path,
                        end: Direction::Right,
                    })
                }
                Ordering::Greater => {
                    next_pointer = node.left();
                }
                Ordering::Less => {
                    next_pointer = node.right();
                }
            }
        }

        None
    }

    /// Gets an iterator over all values in the tree in ascending order.
    pub fn iter<'a>(&'a self) -> Iter<'a, T, A> {
        let mut iter = Iter {
            root: self.root.as_ref().map(|n| n.as_ref()),
            path: vec![],
            end: Direction::Left,
        };

        iter.next();
        iter
    }

    /// Creates an iterator which starts at the first value in the tree with
    /// 'value >= query'.
    ///
    /// The comparator must preserve the ordering of adjacent values as was used
    /// during insertion. But, this function will still return the correct
    /// result if previously non-equal adjacent values become equal.
    pub fn lower_bound<'a, Q>(&'a self, query: &Q) -> Iter<'a, T, A>
    where
        C: Comparator<T, Q>,
    {
        self.lower_bound_by(query, &self.comparator)
    }

    pub fn lower_bound_by<'a, Q, D>(&'a self, query: &Q, comparator: &D) -> Iter<'a, T, A>
    where
        D: Comparator<T, Q>,
    {
        let mut path = self.allocate_path_buf();
        let mut best_depth = 0;

        let mut next_pointer = self.root.as_ref();

        while let Some(node) = next_pointer {
            path.push(node.as_ref());

            match comparator.compare(node.value(), query) {
                // NOTE: In the Equal case, we could stop early if the comparator is equivalent to
                // the one used for insertion as we only ever insert equal elements into the right
                // subtree. We don't stop early though to allow for slight changes to the
                // comparator.
                Ordering::Greater | Ordering::Equal => {
                    best_depth = path.len();
                    next_pointer = node.left();
                }
                Ordering::Less => {
                    next_pointer = node.right();
                }
            }
        }

        path.truncate(best_depth);

        Iter {
            root: self.root.as_ref().map(|n| n.as_ref()),
            path,
            end: Direction::Right,
        }
    }

    pub fn insert(&mut self, value: T) {
        self.insert_with_attribute(value, A::default())
    }

    /// NOTE: Doesn't support insertion of multiple equal values. < TODO: Check
    /// this
    pub fn insert_with_attribute(&mut self, value: T, value_attribute: A) {
        let new_node = Box::new(AVLNode::new(value, value_attribute, None, None));
        Self::insert_inner(new_node, &mut self.root, &self.comparator);
    }

    /// Inserts a given new_node into the node pointed to be current_pointer.
    ///
    /// Returns whether or not the height of the node pointed to by
    /// current_pointer has changed.
    fn insert_inner(
        new_node: Box<AVLNode<T, A>>,
        current_pointer: &mut Option<Box<AVLNode<T, A>>>,
        comparator: &C,
    ) {
        let current_node = match current_pointer.as_mut() {
            Some(n) => n,
            None => {
                // We hit an empty leaf pointer, so add the node.
                *current_pointer = Some(new_node);
                return;
            }
        };

        let changed = {
            if comparator.compare(current_node.value(), new_node.value()) == Ordering::Greater {
                Self::insert_inner(new_node, current_node.get_mut().left_mut(), comparator)
            } else {
                Self::insert_inner(new_node, current_node.get_mut().right_mut(), comparator)
            }
        };

        Self::repair_subtree(current_node);
    }

    pub fn remove(&mut self, value: &T) -> Option<T> {
        Self::remove_search(value, &mut self.root, &self.comparator)
    }

    /// Attempts to find a node equal to 'value' in the subtree pointed to by
    /// 'current_pointer' and then proceeds to delete it.
    ///
    /// Returns the deleted value (or none if the value wasn't found in the
    /// subtree).
    fn remove_search(
        value: &T,
        current_pointer: &mut Option<Box<AVLNode<T, A>>>,
        comparator: &C,
    ) -> Option<T> {
        let current_node = match current_pointer.as_mut() {
            Some(n) => n,
            // Couldn't find the queried value in the tree.
            None => {
                return None;
            }
        };

        let ord = comparator.compare(current_node.value(), value);

        // Found the queried value. Delete it.
        if ord == Ordering::Equal {
            return Self::remove_node(current_pointer);
        }

        // Otherwise, keep (binary) searching for the value.
        let ret = if ord == Ordering::Greater {
            Self::remove_search(value, current_node.get_mut().left_mut(), comparator)
        } else {
            Self::remove_search(value, current_node.get_mut().right_mut(), comparator)
        };

        Self::repair_subtree(current_node);
        ret
    }

    /// Deletes the node pointed to by 'current_pointer'.
    /// current_pointer MUST be Some(_).
    fn remove_node(current_pointer: &mut Option<Box<AVLNode<T, A>>>) -> Option<T> {
        let current_node = current_pointer.as_mut().unwrap();

        if current_node.left().is_none() && current_node.right().is_none() {
            // This node has no children so delete itself in the parent.
            let n = current_pointer.take().unwrap();
            return Some(n.into_value());
        } else if current_node.left().is_none() {
            // Replace with right child.
            let mut n = current_pointer.take().unwrap();
            *current_pointer = n.take_right();
            return Some(n.into_value());
        } else if current_node.right().is_none() {
            // Replace with left child
            let mut n = current_pointer.take().unwrap();
            *current_pointer = n.take_left();
            return Some(n.into_value());
        } else {
            // Both the left and right child are occupied.
            // Replace the current node with the successor.
            let ret = {
                let mut current_node_guard = current_node.get_mut();
                let (current_value, current_attribute, current_right) =
                    current_node_guard.value_right_mut();
                Self::remove_swap_successor(current_value, current_attribute, current_right)
            };
            Self::repair_subtree(current_node);
            ret
        }
    }

    /// Deletes a 'value' in a parent node by swapping it with the smallest
    /// value found in the 'current_pointer' subtree (and then deleting that
    /// smallest value's original node).
    fn remove_swap_successor(
        value: &mut T,
        value_attribute: &mut A,
        current_pointer: &mut Option<Box<AVLNode<T, A>>>,
    ) -> Option<T> {
        let current_node = current_pointer.as_mut().unwrap();

        if current_node.left().is_some() {
            let ret = Self::remove_swap_successor(
                value,
                value_attribute,
                current_node.get_mut().left_mut(),
            );
            Self::repair_subtree(current_node);
            ret
        } else {
            core::mem::swap(value, current_node.value_mut());

            // TODO: If the attribute is trivial then we don't need to perform sub-tree
            // recalculations with it.
            core::mem::swap(
                value_attribute,
                current_node.get_mut().value_attribute_mut(),
            );

            Self::remove_node(current_pointer)
        }
    }

    /// Returns whether or not a height change has occured.
    fn repair_subtree(node: &mut Box<AVLNode<T, A>>) {
        let balance_factor = node.balance_factor();

        if balance_factor == 2 {
            {
                let mut node_guard = node.get_mut();
                let right_child = node_guard.right_mut().as_mut().unwrap();

                if right_child.balance_factor() < 0 {
                    Self::rotate(right_child, Direction::Right);
                }
            }
            Self::rotate(node, Direction::Left);
        } else if balance_factor == -2 {
            {
                let mut node_guard = node.get_mut();
                let left_child = node_guard.left_mut().as_mut().unwrap();

                if left_child.balance_factor() > 0 {
                    Self::rotate(left_child, Direction::Left);
                }
            }
            Self::rotate(node, Direction::Right);
        }
    }

    /// Performs a rotation around a given node which is the root of a subtree
    /// of the BST. The result will preserve the BST ordering.
    ///
    /// If we rotate Left, then the Right child of the given node will become
    /// the root of the sub-tree.
    fn rotate(node: &mut Box<AVLNode<T, A>>, direction: Direction) {
        if direction == Direction::Left {
            let mut node2 = node.take_right().unwrap();
            core::mem::swap(&mut node2, node);

            node2.set_right(node.take_left());
            node.set_left(Some(node2));
        } else {
            let mut node2 = node.take_left().unwrap();
            core::mem::swap(&mut node2, node);

            node2.set_left(node.take_right());
            node.set_right(Some(node2));
        }
    }
}

#[derive(Clone)]
pub struct Iter<'a, T, A> {
    root: Option<&'a AVLNode<T, A>>,

    path: Vec<&'a AVLNode<T, A>>,

    /// When the path is empty, this is which side of the tree we are on.
    end: Direction,
}

impl<'a, T, A: TreeAttribute> Iterator for Iter<'a, T, A> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let last_node = match self.path.last() {
            Some(n) => *n,
            None => {
                if self.end == Direction::Left {
                    // Find leftmost child
                    let mut current_pointer = self.root.clone();
                    while let Some(node) = current_pointer {
                        self.path.push(node);
                        current_pointer = node.left().map(|n| n.as_ref());
                    }
                }

                return None;
            }
        };

        let value = last_node.value();

        // Update path to point to the current node's successor

        if let Some(right_child) = last_node.right() {
            // The current node has a right sub-tree, so pick the smallest node in that
            // tree.

            let mut next_child = right_child.as_ref();
            self.path.push(next_child);

            while let Some(left_child) = next_child.left() {
                next_child = left_child.as_ref();
                self.path.push(next_child);
            }
        } else {
            // Otherwise, we must find the successor by looking at the parent node.

            let mut current_node = last_node;
            self.path.pop(); // Remove current_node from the path.

            while let Some(parent_node) = self.path.last() {
                let is_left_child = parent_node
                    .left()
                    .map(|node| core::ptr::eq(node.as_ref(), current_node))
                    .unwrap_or(false);

                // If we just finished visiting the left subtree of the parent_node, then the
                // next in-order node is the parent_node itself.
                if is_left_child {
                    break;
                }

                // Otherwise, we just finished visiting the right subtree of
                // parent_node so we need to keep going up.
                current_node = *parent_node;
                self.path.pop();
            }
        }

        Some(value)
    }
}

impl<'a, T, A: TreeAttribute> Iter<'a, T, A> {
    /// Views the value at the current position in the tree.
    ///
    /// This returns the same as prev() or next() except doesn't change the
    /// position afterwards.
    pub fn peek(&self) -> Option<&'a T> {
        let last_node = match self.path.last() {
            Some(n) => *n,
            None => {
                return None;
            }
        };

        let value = last_node.value();
        Some(value)
    }

    pub fn prev(&mut self) -> Option<&'a T> {
        // TODO: Support getting the previous node when at the end of the tree.

        let last_node = match self.path.last() {
            Some(n) => *n,
            None => {
                if self.end == Direction::Right {
                    // Find rightmost child
                    let mut current_pointer = self.root.clone();
                    while let Some(node) = current_pointer {
                        self.path.push(node);
                        current_pointer = node.right().map(|n| n.as_ref());
                    }
                }

                return None;
            }
        };

        let value = last_node.value();

        // Update path to point to the current node's predecessor

        if let Some(left_child) = last_node.left() {
            // Pick the largest value in the node's left child.

            let mut next_child = left_child.as_ref();
            self.path.push(next_child);

            while let Some(right_child) = next_child.right() {
                next_child = right_child.as_ref();
                self.path.push(next_child);
            }
        } else {
            // Otherwise find predecessor in the parent.

            let mut current_node = last_node;
            self.path.pop(); // Remove current_node from the path.

            while let Some(parent_node) = self.path.last() {
                if parent_node.right_child_is(current_node) {
                    break;
                }

                current_node = *parent_node;
                self.path.pop();
            }
        }

        Some(value)
    }

    /// Returns the sum of all attributes left of the current node pointed to by
    /// this iterator. This is a log(N) operator for N nodes in the tree.
    pub fn left_attributes(&self) -> A {
        let mut total = A::default();

        for i in 0..self.path.len() {
            let node = self.path[i];

            if i + 1 < self.path.len() {
                let child_node = self.path[i + 1];
                if node.right_child_is(child_node) {
                    if let Some(left) = node.left() {
                        total += left.subtree_attributes();
                    }

                    total += node.value_attribute();
                }
            } else {
                if let Some(left) = node.left() {
                    total += left.subtree_attributes();
                }
            }
        }

        total
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_right_balancing_tests() {
        let mut tree = AVLTree::default();

        tree.insert(10);
        assert_eq!(
            tree.root,
            Some(AVLNode::new(10, EmptyAttribute::default(), None, None).into())
        );

        tree.insert(20);
        assert_eq!(
            tree.root,
            Some(
                AVLNode::new(
                    10,
                    EmptyAttribute::default(),
                    None,
                    Some(AVLNode::new(20, EmptyAttribute::default(), None, None).into())
                )
                .into()
            )
        );

        // Must perform a left rotation at the root.
        tree.insert(30);
        assert_eq!(
            tree.root,
            Some(
                AVLNode::new(
                    20,
                    EmptyAttribute::default(),
                    Some(AVLNode::new(10, EmptyAttribute::default(), None, None).into()),
                    Some(AVLNode::new(30, EmptyAttribute::default(), None, None).into())
                )
                .into()
            )
        );

        tree.insert(25);
        tree.insert(27);

        assert_eq!(
            tree.root,
            Some(
                AVLNode::new(
                    20,
                    EmptyAttribute::default(),
                    Some(AVLNode::new(10, EmptyAttribute::default(), None, None).into()),
                    Some(
                        AVLNode::new(
                            27,
                            EmptyAttribute::default(),
                            Some(AVLNode::new(25, EmptyAttribute::default(), None, None).into()),
                            Some(AVLNode::new(30, EmptyAttribute::default(), None, None).into())
                        )
                        .into()
                    )
                )
                .into()
            )
        );
    }

    #[test]
    fn works() {
        let mut tree = AVLTree::default();

        for i in 0..100 {
            tree.insert(i);
        }

        {
            let mut iter = tree.find(&0).unwrap();
            for i in 0..100 {
                assert_eq!(iter.next(), Some(&i));
            }

            assert_eq!(iter.next(), None);
        }

        {
            let mut iter = tree.find(&99).unwrap();
            for i in (0..100).rev() {
                assert_eq!(iter.prev(), Some(&i));
            }

            assert_eq!(iter.prev(), None);
        }
    }

    #[test]
    fn lower_bound() {
        let mut tree = AVLTree::default();
        tree.insert(10);
        tree.insert(50);
        tree.insert(25);
        tree.insert(30);
        tree.insert(5);

        {
            let mut iter = tree.lower_bound(&20);
            assert_eq!(iter.next(), Some(&25));
            assert_eq!(iter.next(), Some(&30));
            assert_eq!(iter.next(), Some(&50));
            assert_eq!(iter.next(), None);
        }

        {
            let mut iter = tree.lower_bound(&51);
            assert_eq!(iter.next(), None);
            assert_eq!(iter.prev(), None);
            assert_eq!(iter.prev(), Some(&50));
            assert_eq!(iter.peek(), Some(&30));
        }

        {
            let mut iter = tree.lower_bound(&5);
            assert_eq!(iter.next(), Some(&5));
            assert_eq!(iter.next(), Some(&10));
            assert_eq!(iter.next(), Some(&25));
            assert_eq!(iter.next(), Some(&30));
        }
    }

    #[test]
    fn remove_from_start() {
        let mut tree = AVLTree::default();

        for i in 0..100 {
            tree.insert(i);
        }

        for i in 0..100 {
            assert!(tree.find(&i).is_some());
            tree.remove(&i);
            assert!(tree.find(&i).is_none());

            let mut iter = tree.iter();
            for j in (i + 1)..100 {
                assert_eq!(iter.next(), Some(&j));
            }
            assert_eq!(iter.next(), None);
        }
    }

    #[test]
    fn tree_attribute() {
        // Testing the tree attribute feature by using it to store a counter of how many
        // nodes there are in the subtree. So we can query how many nodes are to the
        // left or right of any node.

        let mut tree = AVLTree::<usize, usize, OrdComparator>::new(OrdComparator {});

        tree.insert_with_attribute(10, 1);
        tree.insert_with_attribute(20, 1);
        tree.insert_with_attribute(30, 1);
        tree.insert_with_attribute(15, 1);
        tree.insert_with_attribute(25, 1);

        let iter = tree.find(&30).unwrap();
        assert_eq!(iter.left_attributes(), 4);

        let iter = tree.find(&20).unwrap();
        assert_eq!(iter.left_attributes(), 2);
    }
}
