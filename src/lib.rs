#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod node;

use node::{Node, NodeVariant, NodeVariantMut};

pub struct BTreeVec<T, const B: usize, const C: usize> {
    // TODO: maybe add these
    // first_leaf: Option<()>,
    // last_leaf: Option<()>,
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
                NodeVariant::Internal { children } => {
                    for child in children.iter().flatten() {
                        if index < child.len() {
                            cur_node = child;
                            break;
                        }
                        index -= child.len();
                    }
                }
                NodeVariant::Leaf { values } => {
                    return values.get(index);
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
            // out of bounds
            todo!();
        }

        if let Some(mut root) = self.root_node.take() {
            if let Some(new_node) = root.insert(index, value) {
                self.root_node = Some(Node::from_child_array([root, new_node]));
            }
        } else {
            self.root_node = Some(Node::from_value(value));
        }
    }
}

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
    }
}
