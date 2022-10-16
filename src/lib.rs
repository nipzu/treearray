#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::module_name_repetitions)]
// TODO #![deny(missing_docs)]

extern crate alloc;

use core::{fmt, mem::size_of};

mod cursor;
pub mod iter;
mod node;
mod ownership;
mod panics;
mod utils;

pub use cursor::{Cursor, CursorMut};

use iter::{Drain, Iter};
use node::{
    handle::{InternalMut, InternalRef, LeafMut, LeafRef},
    NodePtr,
};

pub fn foo(b: &BVec<i32>, x: usize) -> Option<&i32> {
    b.get(x)
}

pub struct BVec<T> {
    root: Option<NodePtr<T>>,
}

impl<T> BVec<T> {
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self { root: None }
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        if size_of::<T>() == 0 {
            return self.root.map_or(0, |p| p.as_ptr() as usize);
        }

        self.root.map_or(0, |r| unsafe {
            if r.as_ref().height() == 0 {
                LeafRef::new(r).len()
            } else {
                InternalRef::new(r).len()
            }
        })
    }

    fn height(&self) -> u8 {
        self.root.map_or(0, |r| unsafe { r.as_ref().height() })
    }

    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn get(&self, mut index: usize) -> Option<&T> {
        let mut cur_node = match self.root {
            Some(root) if index < self.len() => root,
            _ => return None,
        };

        for _ in 0..self.height() {
            let handle = unsafe { InternalRef::new(cur_node) };
            cur_node = unsafe { handle.child_containing_index(&mut index) };
        }

        // SAFETY: the height of `cur_node` is 0
        let leaf = unsafe { LeafRef::new(cur_node) };
        // SAFETY: from `get_child_containing_index` we know that index < leaf.len()
        unsafe { Some(leaf.value_unchecked(index)) }
    }

    #[must_use]
    pub fn get_mut(&mut self, mut index: usize) -> Option<&mut T> {
        let mut cur_node = match self.root {
            Some(root) if index < self.len() => root,
            _ => return None,
        };

        // decrement the height of `cur_node` `self.height() - 1` times
        while unsafe { cur_node.as_ref().height() > 0 } {
            let handle = unsafe { InternalMut::new(cur_node) };
            cur_node = unsafe { handle.into_child_containing_index(&mut index) };
        }

        // SAFETY: the height of `cur_node` is 0
        let leaf = unsafe { LeafMut::new(cur_node) };
        // SAFETY: from `into_child_containing_index` we know that index < leaf.len()
        unsafe { Some(leaf.into_value_unchecked_mut(index)) }
    }

    #[must_use]
    pub fn first(&self) -> Option<&T> {
        let mut cur_node = self.root?;

        while unsafe { cur_node.as_ref().height() > 0 } {
            let mut handle = unsafe { InternalRef::new(cur_node) };
            cur_node = unsafe { (*handle.internal_ptr()).children[0].assume_init() };
        }

        unsafe { Some(LeafRef::<T>::new(cur_node).value_unchecked(0)) }
    }

    #[must_use]
    pub fn first_mut(&mut self) -> Option<&mut T> {
        let mut cur_node = self.root?;

        while unsafe { cur_node.as_ref().height() > 0 } {
            let mut handle = unsafe { InternalMut::new(cur_node) };
            cur_node = unsafe { (*handle.internal_ptr()).children[0].assume_init() };
        }

        unsafe { Some(LeafMut::<T>::new(cur_node).into_value_unchecked_mut(0)) }
    }

    #[must_use]
    pub fn last(&self) -> Option<&T> {
        let mut cur_node = self.root?;

        while unsafe { cur_node.as_ref().height() > 0 } {
            let mut handle = unsafe { InternalRef::new(cur_node) };
            let len_children = handle.len_children();
            cur_node = unsafe { (*handle.internal_ptr()).children[len_children - 1].assume_init() };
        }

        let leaf = unsafe { LeafRef::<T>::new(cur_node) };
        let len_values = leaf.len();
        unsafe { Some(leaf.value_unchecked(len_values - 1)) }
    }

    #[must_use]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        let mut cur_node = self.root?;

        while unsafe { cur_node.as_ref().height() > 0 } {
            let mut handle = unsafe { InternalMut::new(cur_node) };
            let len_children = handle.len_children();
            cur_node = unsafe { (*handle.internal_ptr()).children[len_children - 1].assume_init() };
        }

        let leaf = unsafe { LeafMut::<T>::new(cur_node) };
        let len_values = leaf.len();
        unsafe { Some(leaf.into_value_unchecked_mut(len_values - 1)) }
    }

    #[inline]
    pub fn push_front(&mut self, value: T) {
        self.insert(0, value);
    }

    #[inline]
    pub fn push_back(&mut self, value: T) {
        self.insert(self.len(), value);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.drain();
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
    pub fn iter(&self) -> Iter<T> {
        unsafe { Iter::new(self, 0, self.len()) }
    }

    pub fn drain(&mut self) -> Drain<T> {
        Drain::new(self)
    }

    #[must_use]
    pub fn cursor_at(&self, index: usize) -> Cursor<T> {
        Cursor::new(self, index)
    }

    #[must_use]
    pub fn cursor_at_mut(&mut self, index: usize) -> CursorMut<T> {
        CursorMut::new(self, index)
    }
}

impl<T> Drop for BVec<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T> Default for BVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for BVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        const _: BVec<i32> = BVec::new();
        let _ = BVec::<usize>::new();
    }

    #[test]
    fn test_new_zst() {
        const _: BVec<()> = BVec::new();
        let _ = BVec::<()>::new();
    }

    #[test]
    fn test_bvec_size() {
        use core::mem::size_of;

        assert_eq!(size_of::<BVec<i32>>(), size_of::<*mut ()>())
    }

    #[test]
    fn test_push_front_back() {
        let mut b = BVec::<i32>::new();
        let mut l = 0;
        for x in 0..500 {
            b.push_back(x);
            l += 1;
            assert_eq!(l, b.len());
        }

        for x in (-500..0).rev() {
            b.push_front(x);
            l += 1;
            assert_eq!(l, b.len());
        }

        for (a, b) in b.iter().zip(-500..) {
            assert_eq!(*a, b);
        }
    }

    #[test]
    fn test_push_pop_front_back() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut b = BVec::<i32>::new();
        let mut v = Vec::new();
        for x in 0..1000 {
            if rng.gen() {
                b.push_back(x);
                v.push(x);
            } else {
                b.push_front(x);
                v.insert(0, x);
            }
        }

        for _ in 0..1000 {
            let (x, y) = if rng.gen() {
                (b.remove(b.len() - 1), v.pop().unwrap())
            } else {
                (b.remove(0), v.remove(0))
            };
            assert_eq!(x, y);
            assert_eq!(b.len(), v.len());
        }
    }

    #[test]
    fn test_random_insertions() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_4_5 = BVec::<i32>::new();
        let mut b_5_4 = BVec::<i32>::new();

        for x in 0..1000 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, x);
            b_4_5.insert(index, x);
            b_5_4.insert(index, x);
            assert_eq!(v.len(), b_4_5.len());
            assert_eq!(v.len(), b_5_4.len());
        }

        assert_eq!(v, b_4_5.iter().copied().collect::<Vec<_>>());
        assert_eq!(v, b_5_4.iter().copied().collect::<Vec<_>>());
    }

    #[test]
    fn test_random_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_4_2 = BVec::<i32>::new();
        let mut b_5_1 = BVec::<i32>::new();

        for x in 0..1000 {
            v.push(x);
            b_4_2.push_back(x);
            b_5_1.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len());
            let v_rem = v.remove(index);
            let b_4_2_rem = b_4_2.remove(index);
            let b_5_1_rem = b_5_1.remove(index);
            assert_eq!(v.len(), b_4_2.len());
            assert_eq!(v.len(), b_5_1.len());
            assert_eq!(v_rem, b_5_1_rem);
            assert_eq!(v_rem, b_4_2_rem);
        }

        assert!(b_4_2.is_empty());
        assert!(b_5_1.is_empty());
    }

    #[test]
    fn test_random_double_insertions() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_7_3 = BVec::<i32>::new();
        let mut b_5_5 = BVec::<i32>::new();

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
    #[should_panic]
    fn test_remove_past_end() {
        let mut v = BVec::<i32>::new();
        for x in 0..10 {
            v.push_back(x);
        }
        let mut c = v.cursor_at_mut(10);
        c.remove();
    }

    #[test]
    #[should_panic]
    fn test_remove_past_end_root() {
        let mut v = BVec::<i32>::new();
        for x in 0..10 {
            v.push_back(x);
        }
        let mut c = v.cursor_at_mut(10);
        c.remove();
    }

    #[test]
    fn test_random_removals2() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_4_4 = BVec::<i32>::new();
        let mut b_5_5 = BVec::<i32>::new();

        for x in 0..1000 {
            v.push(x);
            b_4_4.push_back(x);
            b_5_5.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len());
            let v_rem = v.remove(index);
            let b_4_4_rem = b_4_4.remove(index);
            let b_5_5_rem = b_5_5.remove(index);
            assert_eq!(v.len(), b_4_4.len());
            assert_eq!(v.len(), b_5_5.len());
            assert_eq!(v_rem, b_5_5_rem);
            assert_eq!(v_rem, b_4_4_rem);
        }
        assert!(b_4_4.is_empty());
        assert!(b_5_5.is_empty());
    }

    #[test]
    fn test_bvec_debug() {
        use alloc::format;
        use alloc::vec::Vec;

        let v = Vec::from_iter(0..1000);
        let mut b = BVec::new();
        for x in 0..1000 {
            b.push_back(x);
        }

        assert_eq!(format!("{v:?}"), format!("{b:?}"));
    }

    #[test]
    fn test_random_double_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);

        let mut v = Vec::new();
        let mut b_4_4 = BVec::<i32>::new();
        let mut b_5_5 = BVec::<i32>::new();

        for x in 0..1000 {
            v.push(x);
            b_4_4.push_back(x);
            b_5_5.push_back(x);
        }

        while !v.is_empty() {
            let index = rng.gen_range(0..v.len() - 1);
            let v1 = v.remove(index);
            let v2 = v.remove(index);
            {
                let mut cursor_5_5 = b_5_5.cursor_at_mut(index);
                let b1 = cursor_5_5.remove();
                let b2 = cursor_5_5.remove();
                assert_eq!(b1, v1);
                assert_eq!(b2, v2);
                assert_eq!(v.len(), b_5_5.len());
            }
            {
                let mut cursor_4_4 = b_4_4.cursor_at_mut(index);
                let b1 = cursor_4_4.remove();
                let b2 = cursor_4_4.remove();
                assert_eq!(b1, v1);
                assert_eq!(b2, v2);
                assert_eq!(v.len(), b_4_4.len());
            }
        }
    }

    #[test]
    fn test_random_cursor_move_right() {
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);
        let mut b = BVec::<i32>::new();
        let n = 1000;

        for x in 0..n as i32 {
            b.push_back(x);
        }

        for _ in 0..n {
            let (start, end) = (rng.gen_range(0..b.len()), rng.gen_range(0..b.len()));

            let mut c = b.cursor_at_mut(start);
            c.move_(end as isize - start as isize);
            assert_eq!(Some(&(end as i32)), c.get());
        }
    }

    #[test]
    fn test_random_cursor_get() {
        let mut b_4_4 = BVec::<i32>::new();
        let mut b_5_5 = BVec::<i32>::new();
        let n = 1000;

        for x in 0..n as i32 {
            b_4_4.push_back(x);
            b_5_5.push_back(x);
        }

        for i in 0..n {
            let x = *b_4_4.get(i).unwrap();
            let y = *b_5_5.get(i).unwrap();

            assert_eq!(x, y);
            assert_eq!(x, *b_4_4.cursor_at_mut(i).get().unwrap());
            assert_eq!(y, *b_5_5.cursor_at_mut(i).get().unwrap());
        }
    }

    #[test]
    fn test_bvec_iter() {
        let mut b = BVec::new();
        for x in 0..1000 {
            b.push_back(x);
        }

        assert!(b.iter().copied().eq(0..1000));
    }

    #[test]
    fn test_empty_cursor() {
        let mut bvec = BVec::<i32>::new();
        let cursor = bvec.cursor_at_mut(0);
        assert!(cursor.get().is_none());
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
        fn foo<'a>(_x: BVec<&'a i32>, _y: &'a i32) {}
        fn _assert_covariant<'b, 'a: 'b>(x: BVec<&'a i32>) -> BVec<&'b i32> {
            x
        }

        let x = BVec::<&'static i32>::new();
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
