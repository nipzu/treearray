#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
// TODO #![deny(missing_docs)]

extern crate alloc;

use core::{fmt, marker::PhantomData, ptr::NonNull};

mod cursor;
pub mod iter;
mod node;
mod panics;
mod utils;

pub use cursor::CursorMut;

use iter::{Drain, Iter};
use node::{DynNode, DynNodeMut, Node, Variant, VariantMut};

// CONST INVARIANTS:
// - `B >= 5`
// - `C % 2 == 1`, which implies `C >= 1`
pub struct BTreeVec<T, const B: usize = 63, const C: usize = 63> {
    root: Option<Root<T, B, C>>,
    // TODO: is this even needed?
    _marker: PhantomData<T>,
}

struct Root<T, const B: usize, const C: usize> {
    height: usize,
    node: Node<T, B, C>,
}

impl<T, const B: usize, const C: usize> Root<T, B, C> {
    const fn as_dyn(&self) -> DynNode<T, B, C> {
        unsafe { DynNode::new(self.height, &self.node) }
    }

    fn as_dyn_mut(&mut self) -> DynNodeMut<T, B, C> {
        unsafe { DynNodeMut::new(self.height, &mut self.node) }
    }
}

impl<T, const B: usize, const C: usize> BTreeVec<T, B, C> {
    /// # Panics
    /// Panics if any of
    /// - `B < 5`,
    /// - `C` is even,
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        // Each (underfull) internal node has to have at least
        // two children to be considered an internal node.
        assert!(B >= 5); // FIXME: that thing in remove

        // If the root consist of 2 leaves of size `C/2`,
        // then it is also considered to be a leaf.
        // Also takes care of the `C == 0` case.
        assert!(C % 2 == 1);

        Self {
            root: None,
            _marker: PhantomData,
        }
    }

    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        match self.root.as_ref() {
            Some(root) => root.node.len(),
            None => 0,
        }
    }

    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn get(&self, mut index: usize) -> Option<&T> {
        if index >= self.len() {
            return None;
        }

        let mut cur_node = self.root.as_ref()?.as_dyn();

        'd: loop {
            match cur_node.variant() {
                Variant::Internal { handle } => {
                    for child in handle.children() {
                        if index < child.len() {
                            cur_node = child;
                            continue 'd;
                        }
                        index -= child.len();
                    }
                    unreachable!();
                }
                Variant::Leaf { handle } => {
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

        let mut cur_node = self.root.as_mut()?.as_dyn_mut();

        'd: loop {
            match cur_node.into_variant_mut() {
                VariantMut::Internal { handle } => {
                    for child in handle.into_children_mut() {
                        if index < child.len() {
                            cur_node = child;
                            continue 'd;
                        }
                        index -= child.len();
                    }
                    unreachable!();
                }
                VariantMut::Leaf { handle } => {
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
    pub fn first_mut(&mut self) -> Option<&mut T> {
        self.get_mut(0)
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

    // TODO: probably needs rework
    pub fn clear(&mut self) {
        self.root = None;
    }

    /// # Panics
    /// Panics if `index > self.len()`.
    pub fn insert(&mut self, index: usize, value: T) {
        self.cursor_at_mut(index).insert(value);
    }

    /// # Panics
    /// Panics if `index >= self.len()`.
    pub fn remove(&mut self, index: usize) -> T {
        self.cursor_at_mut(index).remove()
    }

    #[must_use]
    pub const fn iter(&self) -> Iter<T, B, C> {
        Iter::new(self)
    }

    pub fn drain(&mut self) -> Drain<T, B, C> {
        Drain::new(self)
    }

    // #[must_use]
    // pub fn cursor_at(&self, mut index: usize) -> Cursor<T, B, C> {
    //     todo!()
    // }

    #[must_use]
    pub fn cursor_at_mut(&mut self, index: usize) -> CursorMut<T, B, C> {
        CursorMut::new_at(NonNull::from(&mut self.root), index)
    }
}

impl<T, const B: usize, const C: usize> Drop for BTreeVec<T, B, C> {
    fn drop(&mut self) {
        self.drain();
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
        let mut b = BTreeVec::<i32, 7, 5>::new();
        for x in 0..500 {
            b.push_back(x);
        }

        for x in (-500..0).rev() {
            b.push_front(x)
        }

        for (a, b) in b.iter().zip(-500..) {
            assert_eq!(*a, b);
        }
    }

    #[test]
    fn test_insert_front_back2() {
        let mut b = BTreeVec::<i32, 5, 5>::new();
        for x in 0..20 {
            b.push_back(x);
        }

        for x in (-20..0).rev() {
            b.push_front(x)
        }

        for (a, b) in b.iter().zip(-20..) {
            assert_eq!(*a, b);
        }
    }

    #[test]
    fn test_random_insertions() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_7_3 = BTreeVec::<i32, 7, 3>::new();
        let mut b_5_1 = BTreeVec::<i32, 5, 1>::new();

        for x in 0..1000 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, x);
            b_7_3.insert(index, x);
            b_5_1.insert(index, x);
            assert_eq!(v.len(), b_7_3.len());
            assert_eq!(v.len(), b_5_1.len());
        }

        assert_eq!(v, b_7_3.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_1.iter().copied().collect::<Vec<_>>());
    }

    #[test]
    fn test_random_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        // let mut b_3_3 = BTreeVec::<i32, 3, 3>::new();
        let mut b_5_1 = BTreeVec::<i32, 5, 1>::new();

        for x in 0..1000 {
            v.push(x);
            // b_3_3.push_back(x);
            b_5_1.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len());
            let v_rem = v.remove(index);
            // b_3_3.remove(index);
            let b_5_1_rem = b_5_1.remove(index);
            // assert_eq!(v.len(), b_3_3.len());
            assert_eq!(v.len(), b_5_1.len());
            assert_eq!(v_rem, b_5_1_rem);
        }

        assert!(b_5_1.is_empty());
    }

    #[test]
    fn test_random_double_insertions() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_7_3 = BTreeVec::<i32, 7, 3>::new();
        let mut b_5_5 = BTreeVec::<i32, 5, 5>::new();

        for x in 0..500 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, 2 * x);
            v.insert(index, 2 * x + 1);
            let mut cursor_7_3 = b_7_3.cursor_at_mut(index);
            let mut cursor_5_5 = b_5_5.cursor_at_mut(index);
            cursor_7_3.insert(2 * x);
            cursor_7_3.insert(2 * x + 1);
            cursor_5_5.insert(2 * x);
            cursor_5_5.insert(2 * x + 1);
            assert_eq!(v.len(), b_7_3.len());
            assert_eq!(v.len(), b_5_5.len());
        }

        assert_eq!(v, b_7_3.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_5.iter().copied().collect::<Vec<_>>());
    }

    #[test]
    fn test_random_removals2() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        // let mut b_3_3 = BTreeVec::<i32, 3, 3>::new();
        let mut b_5_5 = BTreeVec::<i32, 5, 5>::new();

        for x in 0..1000 {
            v.push(x);
            // b_3_3.push_back(x);
            b_5_5.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len());
            let v_rem = v.remove(index);
            // b_3_3.remove(index);
            let b_5_5_rem = b_5_5.remove(index);
            // assert_eq!(v.len(), b_3_3.len());
            assert_eq!(v.len(), b_5_5.len());
            assert_eq!(v_rem, b_5_5_rem);
        }
        assert!(b_5_5.is_empty());
    }

    #[test]
    fn test_random_double_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_5_5 = BTreeVec::<i32, 5, 5>::new();

        for x in 0..1000 {
            v.push(x);
            b_5_5.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len() - 1);
            let mut cursor = b_5_5.cursor_at_mut(index);
            let v1 = cursor.remove();
            let v2 = cursor.remove();
            assert_eq!(v1, v.remove(index));
            assert_eq!(v2, v.remove(index));
            assert_eq!(v.len(), b_5_5.len());
        }
    }

    // #[test]
    // #[should_panic(expected = "length overflow")]
    // fn test_zst_length_overflow() {
    //     let mut b = BTreeVec::<i32, 10, { usize::MAX / 3 }>::new();
    // }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_cursormut_invariant() {
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/compile_fail/test_cursormut_invariant.rs");
    }

    #[test]
    fn test_bvec_covariant() {
        fn foo<'a>(_x: crate::BTreeVec<&'a i32>, _y: &'a i32) {}

        let x = crate::BTreeVec::<&'static i32>::new();
        let v = 123;
        let r = &v;
        foo(x, r);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_bvec_drop_check() {
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/compile_fail/test_bvec_drop_check.rs");
    }
}
