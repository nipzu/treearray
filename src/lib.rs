#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod iter;
mod node;
mod panics;

use iter::Iter;
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

        'd: loop {
            match cur_node.variant() {
                NodeVariant::Internal { handle } => {
                    for child in handle.children().iter().flatten() {
                        if index < child.len() {
                            cur_node = child;
                            continue 'd;
                        }
                        index -= child.len();
                    }
                    unreachable!();
                }
                NodeVariant::Leaf { handle } => {
                    return handle.values().get(index);
                }
            }
        }
    }

    #[must_use]
    pub fn get_mut(&mut self, mut index: usize) -> Option<&mut T> {
        if index >= self.len() {
            return None;
        }

        let mut cur_node = self.root_node.as_mut()?;

        'd: loop {
            match cur_node.variant_mut() {
                NodeVariantMut::Internal { handle } => {
                    for child in handle.into_children_mut().iter_mut().flatten() {
                        if index < child.len() {
                            cur_node = child;
                            continue 'd;
                        }
                        index -= child.len();
                    }
                    unreachable!();
                }
                NodeVariantMut::Leaf { handle } => {
                    return handle.into_values_mut().get_mut(index);
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

    #[must_use]
    #[inline]
    pub fn first_mut(&mut self) -> Option<&mut T> {
        self.get_mut(0)
    }

    #[must_use]
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        // If `self.len() == 0`, we wrap to `usize::MAX` which
        // is definitely outside the range of an empty array.
        self.get_mut(self.len().wrapping_sub(1))
    }

    #[inline]
    pub fn push_front(&mut self, value: T) {
        self.insert(0, value);
    }

    #[inline]
    pub fn push_back(&mut self, value: T) {
        self.insert(self.len(), value);
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

    pub fn iter(&self) -> Iter<T, B, C> {
        Iter::new(self)
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
    use rand::Rng;

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
        assert_eq!(v.get(3), None);

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
        assert_eq!(v.get(3), None);

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
        assert_eq!(v.get(3), None);

        let mut v = BTreeVec::<(), 4, { usize::MAX }>::new();
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
        assert_eq!(v.get(3), None);
    }

    #[test]
    fn test_insert_2() {
        let mut b = BTreeVec::<_, 4, 5>::new();
        for x in 0..200 {
            b.push_back(x);
        }

        for x in (-200..0).rev() {
            b.push_front(x)
        }

        for (a, b) in b.iter().zip(-200..) {
            assert_eq!(*a, b);
        }
    }

    #[test]
    fn test_random_insertions() {
        let mut rng = rand::thread_rng();

        let mut v = alloc::vec::Vec::new();
        let mut b_3_3 = BTreeVec::<i32, 3, 3>::new();
        let mut b_5_1 = BTreeVec::<i32, 5, 1>::new();

        for x in 0..1000 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, x);
            b_3_3.insert(index, x);
            b_5_1.insert(index, x);
        }

        for (a, b) in v.iter().zip(b_3_3.iter()) {
            assert_eq!(a, b);
        }
        for (a, b) in v.iter().zip(b_5_1.iter()) {
            assert_eq!(a, b);
        }
    }

    // #[test]
    // #[should_panic(expected = "length overflow")]
    // fn test_zst_length_overflow() {
    //     let mut b = BTreeVec::<i32, 10, { usize::MAX / 3 }>::new();
    // }
}
