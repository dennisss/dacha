use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::tree::avl_node::AVLNode;

pub trait InsertComparator<T> = Fn(&T, &T) -> Ordering;

pub trait QueryComparator<T, Q> = Fn(&T, &Q) -> Ordering;

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
pub struct AVLTree<T> {
    root: Option<Box<AVLNode<T>>>,
}

impl<T> AVLTree<T> {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Creates an iterator which starts at the first value in the tree with
    /// 'value >= query'.
    ///
    /// The comparator must preserve the ordering of adjacent values as was used
    /// during insertion. But, this function will still return the correct
    /// result if previously non-equal adjacent values become equal.
    pub fn lower_bound_by<'a, Q>(
        &'a self,
        query: &Q,
        comparator: &dyn QueryComparator<T, Q>,
    ) -> Iter<'a, T> {
        let mut path = vec![];
        let mut best_depth = 0;

        let mut next_pointer = self.root.as_ref();

        while let Some(node) = next_pointer {
            path.push(node.as_ref());

            match comparator(node.value(), query) {
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

        Iter { path }
    }

    /// NOTE: Doesn't support insertion of multiple equal values.
    pub fn insert_by(&mut self, value: T, comparator: &dyn InsertComparator<T>) {
        let new_node = Box::new(AVLNode::new(value, None, None));

        Self::insert_inner(new_node, &mut self.root, comparator);
    }

    /// Inserts a given new_node into the node pointed to be current_pointer.
    ///
    /// Returns whether or not the height of the node pointed to by
    /// current_pointer has changed.
    fn insert_inner(
        new_node: Box<AVLNode<T>>,
        current_pointer: &mut Option<Box<AVLNode<T>>>,
        comparator: &dyn InsertComparator<T>,
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
            if comparator(current_node.value(), new_node.value()) == Ordering::Greater {
                Self::insert_inner(new_node, current_node.left_mut(), comparator)
            } else {
                Self::insert_inner(new_node, current_node.right_mut(), comparator)
            }
        };

        Self::repair_subtree(current_node);
    }

    pub fn remove_by(&mut self, value: &T, comparator: &dyn InsertComparator<T>) -> Option<T> {
        Self::remove_search(value, &mut self.root, comparator)
    }

    /// Attempts to find a node equal to 'value' in the subtree pointed to by
    /// 'current_pointer' and then proceeds to delete it.
    ///
    /// Returns the deleted value (or none if the value wasn't found in the
    /// subtree).
    fn remove_search(
        value: &T,
        current_pointer: &mut Option<Box<AVLNode<T>>>,
        comparator: &dyn InsertComparator<T>,
    ) -> Option<T> {
        let current_node = match current_pointer.as_mut() {
            Some(n) => n,
            // Couldn't find the queried value in the tree.
            None => {
                return None;
            }
        };

        let ord = comparator(current_node.value(), value);

        // Found the queried value. Delete it.
        if ord == Ordering::Equal {
            return Self::remove_node(current_pointer);
        }

        // Otherwise, keep (binary) searching for the value.
        let ret = if ord == Ordering::Greater {
            Self::remove_search(value, current_node.left_mut(), comparator)
        } else {
            Self::remove_search(value, current_node.right_mut(), comparator)
        };

        Self::repair_subtree(current_node);
        ret
    }

    /// Deletes the node pointed to by 'current_pointer'.
    /// current_pointer MUST be Some(_).
    fn remove_node(current_pointer: &mut Option<Box<AVLNode<T>>>) -> Option<T> {
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
            let (current_value, current_right) = current_node.value_right_mut();
            let ret = Self::remove_swap_successor(current_value, current_right);
            Self::repair_subtree(current_node);
            ret
        }
    }

    /// Deletes a 'value' in a parent node by swapping it with the smallest
    /// value found in the 'current_pointer' subtree (and then deleting that
    /// smallest value's original node).
    fn remove_swap_successor(
        value: &mut T,
        current_pointer: &mut Option<Box<AVLNode<T>>>,
    ) -> Option<T> {
        let current_node = current_pointer.as_mut().unwrap();

        if current_node.left().is_some() {
            let ret = Self::remove_swap_successor(value, current_node.left_mut());
            Self::repair_subtree(current_node);
            ret
        } else {
            core::mem::swap(value, current_node.value_mut());
            Self::remove_node(current_pointer)
        }
    }

    /// Returns whether or not a height change has occured.
    fn repair_subtree(node: &mut Box<AVLNode<T>>) {
        let balance_factor = node.balance_factor();

        if balance_factor == 2 {
            let right_child = node.right_mut().as_mut().unwrap();

            if right_child.balance_factor() < 0 {
                Self::rotate(right_child, Direction::Right);
            }

            Self::rotate(node, Direction::Left);
        } else if balance_factor == -2 {
            let left_child = node.left_mut().as_mut().unwrap();

            if left_child.balance_factor() > 0 {
                Self::rotate(left_child, Direction::Left);
            }

            Self::rotate(node, Direction::Right);
        }
    }

    /// Performs a rotation around a given node which is the root of a subtree
    /// of the BST. The result will preserve the BST ordering.
    ///
    /// If we rotate Left, then the Right child of the given node will become
    /// the root of the sub-tree.
    fn rotate(node: &mut Box<AVLNode<T>>, direction: Direction) {
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

impl<T: Ord> AVLTree<T> {
    pub fn lower_bound<'a>(&'a self, query: &T) -> Iter<'a, T> {
        self.lower_bound_by(query, &|a, b| a.cmp(b))
    }

    pub fn insert(&mut self, value: T) {
        self.insert_by(value, &|a, b| a.cmp(b))
    }

    pub fn remove(&mut self, value: &T) -> Option<T> {
        self.remove_by(value, &|a, b| a.cmp(b))
    }
}

pub struct Iter<'a, T> {
    path: Vec<&'a AVLNode<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let last_node = match self.path.last() {
            Some(n) => *n,
            None => {
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
                self.path.pop();
            }
        }

        Some(value)
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
        let mut tree = AVLTree::new();

        tree.insert(10);
        assert_eq!(tree.root, Some(AVLNode::new(10, None, None).into()));

        tree.insert(20);
        assert_eq!(
            tree.root,
            Some(AVLNode::new(10, None, Some(AVLNode::new(20, None, None).into())).into())
        );

        // Must perform a left rotation at the root.
        tree.insert(30);
        assert_eq!(
            tree.root,
            Some(
                AVLNode::new(
                    20,
                    Some(AVLNode::new(10, None, None).into()),
                    Some(AVLNode::new(30, None, None).into())
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
                    Some(AVLNode::new(10, None, None).into()),
                    Some(
                        AVLNode::new(
                            27,
                            Some(AVLNode::new(25, None, None).into()),
                            Some(AVLNode::new(30, None, None).into())
                        )
                        .into()
                    )
                )
                .into()
            )
        );
    }

    #[test]
    fn avl_works() {
        let mut tree = AVLTree::new();

        for i in 0..10 {
            tree.insert(i);
        }

        println!("{:#?}", tree);

        {
            let mut iter = tree.lower_bound(&3);
            while let Some(value) = iter.next() {
                println!("Iter: {}", value);
            }
        }

        assert_eq!(tree.remove(&5), Some(5));

        {
            let mut iter = tree.lower_bound(&3);
            while let Some(value) = iter.next() {
                println!("Iter: {}", value);
            }
        }
    }
}
