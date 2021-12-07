#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use core::fmt;
use core::mem::size_of;

pub mod iter;
mod node;
mod panics;

use iter::Iter;
use node::{Node, NodeVariant, NodeVariantMut};
use panics::panic_out_of_bounds;

// CONST INVARIANTS:
// - `B >= 3`
// - `C % 2 == 1`, which implies `C >= 1`
// - `C * size_of<T>() <= isize::MAX`
pub struct BTreeVec<T, const B: usize, const C: usize> {
    // TODO: maybe a depth field?
    root_node: Option<Node<T, B, C>>,
}

impl<T, const B: usize, const C: usize> BTreeVec<T, B, C> {
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        // Each internal node has to have at least two children
        // to be considered an internal node.
        assert!(B >= 3);

        // If the root consist of 2 leaves of size `C/2`,
        // then it is also considered to be a leaf.
        // Also takes care of the `C == 0` case.
        assert!(C % 2 == 1);

        // `slice::from_raw_parts` requires that
        // `len * size_of<T>() <= isize::MAX`
        assert!(C.saturating_mul(size_of::<T>()) <= isize::MAX as usize);

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
        // If `self.len() == 0`, the index wraps to `usize::MAX`
        // which is definitely outside the range of an empty array.
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
        // If `self.len() == 0`, the index wraps to `usize::MAX`
        // which is definitely outside the range of an empty array.
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

    pub fn remove(&mut self, index: usize) -> T {
        if index >= self.len() {
            panic_out_of_bounds(index, self.len());
        }

        let mut root = self.root_node.as_mut().unwrap();

        todo!()
    }

    #[must_use]
    pub const fn iter(&self) -> Iter<T, B, C> {
        Iter::new(self)
    }
}

// TODO: this could maybe be derived in the future
// if const bounds can be checked at compile time
impl<T, const B: usize, const C: usize> Default for BTreeVec<T, B, C> {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: test this
impl<T: fmt::Debug, const B: usize, const C: usize> fmt::Debug for BTreeVec<T, B, C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        const _: BTreeVec<i32, 7, 3> = BTreeVec::new();
    }

    #[test]
    fn test_insert_front_back() {
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
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_3_3 = BTreeVec::<i32, 3, 3>::new();
        let mut b_5_1 = BTreeVec::<i32, 5, 1>::new();

        for x in 0..1000 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, x);
            b_3_3.insert(index, x);
            b_5_1.insert(index, x);
            assert_eq!(v.len(), b_3_3.len());
            assert_eq!(v.len(), b_5_1.len());
        }

        assert_eq!(v, b_3_3.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_1.iter().copied().collect::<Vec<_>>());
    }

    #[test]
    fn test_random_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_3_3 = BTreeVec::<i32, 3, 3>::new();
        let mut b_5_1 = BTreeVec::<i32, 5, 1>::new();

        for x in 0..1000 {
            v.push(x);
            b_3_3.push_back(x);
            b_5_1.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..=v.len());
            v.remove(index);
            b_3_3.remove(index);
            b_5_1.remove(index);
            assert_eq!(v.len(), b_3_3.len());
            assert_eq!(v.len(), b_5_1.len());
        }

        assert_eq!(v, b_3_3.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_1.iter().copied().collect::<Vec<_>>());
    }

    // #[test]
    // #[should_panic(expected = "length overflow")]
    // fn test_zst_length_overflow() {
    //     let mut b = BTreeVec::<i32, 10, { usize::MAX / 3 }>::new();
    // }
}
