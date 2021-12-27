#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
// TODO #![deny(missing_docs)]

extern crate alloc;

use core::fmt;
use core::mem::size_of;
use core::num::NonZeroUsize;
use core::ptr;

pub mod iter;
mod node;
mod panics;
mod utils;

use iter::Iter;
use node::handle::{LeafMut, RemoveResult};
use node::{Node, Variant, VariantMut};
use panics::panic_out_of_bounds;
use utils::slice_shift_left;

pub fn foo(x: &mut BTreeVec<i32, 50, 101>, index: usize) -> Option<&i32> {
    x.get(index)
}

// CONST INVARIANTS:
// - `B >= 5`
// - `C % 2 == 1`, which implies `C >= 1`
// - `C * size_of<T>() <= isize::MAX`
pub struct BTreeVec<T, const B: usize = 63, const C: usize = 63> {
    // TODO: maybe a depth field?
    root_node: Option<Node<T, B, C>>,
}

impl<T, const B: usize, const C: usize> BTreeVec<T, B, C> {
    /// # Panics
    /// Panics if any of
    /// - `B < 5`,
    /// - `C` is even,
    /// - `C * size_of<T>() > isize::MAX`.
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

        #[allow(clippy::checked_conversions)]
        {
            // `slice::from_raw_parts` requires that
            // `len * size_of<T>() <= isize::MAX`
            let arr_len = C.saturating_mul(size_of::<T>());
            assert!(arr_len <= isize::MAX as usize);
        }

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
                Variant::Internal { handle } => {
                    for child in handle.children().iter().flatten() {
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

        let mut cur_node = self.root_node.as_mut()?;

        'd: loop {
            match cur_node.variant_mut() {
                VariantMut::Internal { handle } => {
                    for child in handle.into_children_mut().iter_mut().flatten() {
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

    /// # Panics
    /// Panics if `index > self.len()`.
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

    /// # Panics
    /// Panics if `index >= self.len()`.
    pub fn remove(&mut self, index: usize) -> T {
        if index >= self.len() {
            panic_out_of_bounds(index, self.len());
        }

        match self.root_node.as_mut().unwrap().variant_mut() {
            VariantMut::Internal { mut handle } => {
                if handle.len() == C + 1 {
                    let [fst, snd]: &mut [Option<Node<T, B, C>>; 2] =
                        (&mut handle.children_mut()[..2]).try_into().unwrap();
                    let (mut fst, mut snd) = unsafe {
                        (
                            LeafMut::new(fst.as_mut().unwrap()),
                            LeafMut::new(snd.as_mut().unwrap()),
                        )
                    };

                    let fst_ptr = fst.values_mut().as_mut_ptr();
                    let snd_ptr = snd.values_mut().as_mut_ptr();

                    let ret;
                    if index < fst.len() {
                        unsafe {
                            ret = fst_ptr.add(index).read();
                            ptr::copy(
                                fst_ptr.add(index + 1),
                                fst_ptr.add(index),
                                fst.len() - index - 1,
                            );
                            ptr::copy_nonoverlapping(
                                snd_ptr,
                                fst_ptr.add(fst.len() - 1),
                                snd.len(),
                            );
                        }
                    } else {
                        unsafe {
                            ret = snd_ptr.add(index - fst.len()).read();
                            ptr::copy_nonoverlapping(
                                snd_ptr,
                                fst_ptr.add(fst.len()),
                                index - fst.len(),
                            );
                            ptr::copy_nonoverlapping(
                                snd_ptr.add(index - fst.len()),
                                fst_ptr.add(index),
                                snd.len() - index + fst.len() - 1,
                            );
                        }
                    }

                    unsafe { fst.set_len(C) };
                    self.root_node = handle.into_children_mut()[0].take();

                    ret
                } else {
                    match unsafe { handle.remove(index) } {
                        RemoveResult::Ok(val) => val,
                        RemoveResult::WithVacancy(val, child_index) => {
                            slice_shift_left(&mut handle.children_mut()[child_index..], None);
                            if handle.children()[1].is_none() {
                                self.root_node = handle.children_mut()[0].take();
                            }
                            val
                        }
                    }
                }
            }
            VariantMut::Leaf { mut handle } => {
                if let Some(new_len) = NonZeroUsize::new(handle.len() - 1) {
                    unsafe {
                        let ret = handle.remove_no_underflow(index);
                        handle.set_length(new_len);
                        ret
                    }
                } else {
                    let ret = unsafe { handle.into_values_mut().as_ptr().read() };
                    let old_root = self.root_node.take().unwrap();
                    old_root.free();
                    ret
                }
            }
        }
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
        let mut b = BTreeVec::<i32, 6, 5>::new();
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

        // assert_eq!(v, b_3_3.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_1.iter().copied().collect::<Vec<_>>());
    }

    // #[test]
    // #[should_panic(expected = "length overflow")]
    // fn test_zst_length_overflow() {
    //     let mut b = BTreeVec::<i32, 10, { usize::MAX / 3 }>::new();
    // }
}
