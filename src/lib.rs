#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod node;
mod panics;

use node::{Node, NodeVariant, NodeVariantMut};
use panics::panic_out_of_bounds;

pub struct BTreeVec<T, const B: usize, const C: usize> {
    // TODO: maybe a depth field?
    root_node: Option<Node<T, B, C>>,
}

impl<T, const B: usize, const C: usize> BTreeVec<T, B, C> {
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        // TODO: do something about this runtime check
        assert!(B >= 3);
        assert!(C >= 1);
        Self { root_node: None }
    }

    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        match self.root_node.as_ref() {
            Some(node) => node.len(),
            None => 0,
        }
    }

    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.root_node.is_none()
    }

    #[must_use]
    pub fn get(&self, mut index: usize) -> Option<&T> {
        if index >= self.len() {
            return None;
        }

        let mut cur_node = self.root_node.as_ref()?;

        loop {
            match cur_node.variant() {
                NodeVariant::Internal { handle } => {
                    for child in handle.children().iter().flatten() {
                        if index < child.len() {
                            cur_node = child;
                            break;
                        }
                        index -= child.len();
                    }
                }
                NodeVariant::Leaf { handle } => {
                    return handle.values().get(index);
                }
            }
        }
    }

    #[must_use]
    #[inline]
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }

    #[must_use]
    #[inline]
    pub fn last(&self) -> Option<&T> {
        // If `self.len() == 0`, we wrap to `usize::MAX` which
        // is definitely outside the range of an empty array.
        self.get(self.len().wrapping_sub(1))
    }

    // TODO: should this be inlined?
    pub fn clear(&mut self) {
        self.root_node = None;
    }

    pub fn insert(&mut self, index: usize, value: T) {
        if index > self.len() {
            panic_out_of_bounds(index, self.len());
        }

        self.root_node = if let Some(mut root) = self.root_node.take() {
            if let Some(new_node) = root.insert(index, value) {
                Some(Node::from_child_array([root, new_node]))
            } else {
                Some(root)
            }
        } else {
            Some(Node::from_value(value))
        }
    }
}

// TODO: this could maybe be derived in the future
// if we can check the const bounds at compile time
impl<T, const B: usize, const C: usize> Default for BTreeVec<T, B, C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let _ = BTreeVec::<i32, 7, 2>::new();
    }

    #[test]
    fn test_insert_and_get() {
        let mut v = BTreeVec::<i32, 3, 1>::new();
        assert_eq!(v.len(), 0);

        v.insert(0, 0);
        assert_eq!(v.len(), 1);
        assert_eq!(v.get(0), Some(&0));

        v.insert(1, 1);
        assert_eq!(v.len(), 2);
        assert_eq!(v.get(0), Some(&0));
        assert_eq!(v.get(1), Some(&1));

        v.insert(2, 2);
        assert_eq!(v.len(), 3);
        assert_eq!(v.get(0), Some(&0));
        assert_eq!(v.get(1), Some(&1));
        assert_eq!(v.get(2), Some(&2));

        let mut v = BTreeVec::<i32, 4, 1>::new();
        assert_eq!(v.len(), 0);

        v.insert(0, 3);
        assert_eq!(v.len(), 1);
        assert_eq!(v.get(0), Some(&3));

        v.insert(0, 1);
        assert_eq!(v.len(), 2);
        assert_eq!(v.get(0), Some(&1));
        assert_eq!(v.get(1), Some(&3));

        v.insert(1, 2);
        assert_eq!(v.len(), 3);
        assert_eq!(v.get(0), Some(&1));
        assert_eq!(v.get(1), Some(&2));
        assert_eq!(v.get(2), Some(&3));

        let mut v = BTreeVec::<i32, 4, 3>::new();
        assert_eq!(v.len(), 0);

        v.insert(0, 3);
        assert_eq!(v.len(), 1);

        v.insert(0, 1);
        assert_eq!(v.len(), 2);

        v.insert(1, 2);
        assert_eq!(v.len(), 3);

        assert_eq!(v.get(0), Some(&1));
        assert_eq!(v.get(1), Some(&2));
        assert_eq!(v.get(2), Some(&3));

        let mut v = BTreeVec::<(), 4, 3>::new();
        assert_eq!(v.len(), 0);

        v.insert(0, ());
        assert_eq!(v.len(), 1);

        v.insert(0, ());
        assert_eq!(v.len(), 2);

        v.insert(1, ());
        assert_eq!(v.len(), 3);

        assert_eq!(v.get(0), Some(&()));
        assert_eq!(v.get(1), Some(&()));
        assert_eq!(v.get(2), Some(&()));
    }
}
