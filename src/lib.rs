#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::module_name_repetitions)]
// TODO #![deny(missing_docs)]

extern crate alloc;

use core::{
    fmt,
    hash::{Hash, Hasher},
    mem::MaybeUninit,
    ops::{Index, IndexMut, RangeBounds},
};

mod cursor;
pub mod iter;
mod node;
mod ownership;
mod panics;
mod utils;

pub use cursor::{Cursor, CursorMut, InboundsCursor, InboundsCursorMut};

use iter::{Drain, Iter};
use node::{handle::LeafMut, InternalNode, NodeBase, NodePtr, RawNodeWithLen};
use panics::panic_out_of_bounds;

use crate::node::handle::{height, Internal, InternalMut, Node, Leaf};

pub fn foo<'a>(b: &'a mut BVec<i32>, x: usize, y: i32) {
    b.insert(x, y);
}

pub struct BVec<T> {
    root: MaybeUninit<NodePtr<T>>,
    len: usize,
    height: u16,
}

impl<T> BVec<T> {
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self {
            root: MaybeUninit::uninit(),
            len: 0,
            height: 0,
        }
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    // TODO: should this be pub?
    const fn is_not_empty(&self) -> bool {
        self.len != 0
    }

    fn root(&self) -> Option<NodePtr<T>> {
        self.is_not_empty()
            .then(|| unsafe { self.root.assume_init() })
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        InboundsCursor::try_new(self, index).map(InboundsCursor::get)
    }

    #[must_use]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        InboundsCursorMut::try_new(self, index).map(InboundsCursorMut::into_mut)
    }

    #[must_use]
    pub fn first(&self) -> Option<&T> {
        InboundsCursor::try_new_first(self).map(InboundsCursor::get)
    }

    #[must_use]
    pub fn first_mut(&mut self) -> Option<&mut T> {
        InboundsCursorMut::try_new_first(self).map(InboundsCursorMut::into_mut)
    }

    #[must_use]
    pub fn last(&self) -> Option<&T> {
        InboundsCursor::try_new_last(self).map(InboundsCursor::get)
    }

    #[must_use]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        InboundsCursorMut::try_new_last(self).map(InboundsCursorMut::into_mut)
    }

    #[inline]
    pub fn push_front(&mut self, value: T) {
        self.insert(0, value);
    }

    /*
    #[inline]
    pub fn push_back(&mut self, value: T) {
        CursorInner::new_past_the_end(self).insert(value);
    }*/

    pub fn push_back(&mut self, value: T) {
        self.insert(self.len(), value);
    }

    #[inline]
    pub fn clear(&mut self) {
        //self.drain(..);
    }

    /// # Panics
    /// Panics if `index > self.len()`.
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len());

        if self.is_empty() {
            let new_root = NodeBase::new_leaf();
            unsafe { LeafMut::new(new_root).values_mut().insert(0, value) };
            self.root.write(new_root);
            self.len += 1;
            debug_assert_eq!(self.height, 0);
            return;
        }

        self.len += 1;
        // TODO: if let
        let root = unsafe { self.root.assume_init() };

        unsafe fn insert_to_node<T>(
            node: NodePtr<T>,
            index: usize,
            value: T,
            h: u16,
        ) -> Option<RawNodeWithLen<T>> {
            if h == 0 {
                let mut leaf = unsafe { LeafMut::new(node) };
                leaf.insert_value(index, value)
            } else {
                let mut internal = unsafe { InternalMut::new(node) };
                let (new_index, child_index) = internal
                    .node()
                    .lengths
                    .child_containing_index_inclusive(index);
                unsafe { internal.add_length_wrapping(child_index, 1) };
                let child = unsafe { internal.node_mut().children[child_index].assume_init() };
                unsafe {
                    insert_to_node(child, new_index, value, h - 1)
                        .map(|r| internal.insert_split_of_child(child_index, r))
                        .flatten()
                }
            }
        }

        if let Some(new_node) =
            unsafe { insert_to_node(root, index, value, self.height) }
        {
            let old_root = root;
            let old_root_len = self.len() - new_node.0;
            self.root.write(InternalNode::from_child_array([
                RawNodeWithLen(old_root_len, old_root),
                new_node,
            ]));
            self.height += 1;
        }
    }

    /// # Panics
    /// Panics if `index >= self.len()`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len());

        self.len -= 1;

        unsafe fn remove_from_node<T>(node: NodePtr<T>, index: usize, h: u16) -> T {
            match h {
                0 => {
                    let mut leaf = unsafe { LeafMut::new(node) };
                    leaf.remove_child(index)
                }
                1 => {
                    let mut internal = unsafe { Node::<ownership::Mut, height::One, T>::new(node) };
                    let (new_index, child_index) = internal
                        .node()
                        .lengths
                        .child_containing_index(index);
                    unsafe { internal.add_length_wrapping(child_index, 1_usize.wrapping_neg()) };
                    let child = unsafe { internal.node_mut().children[child_index].assume_init() };
                    let ret = unsafe { remove_from_node(child, new_index, 0) };

                    // TODO: LeafRef
                    if unsafe { LeafMut::new(child).is_underfull() } {
                        if child_index == 0 {
                            internal.handle_underfull_leaf_child_head();
                        } else {
                            internal.handle_underfull_leaf_child_tail(child_index);
                        }
                    }
                    ret
                }
                _ => {
                    let mut internal = unsafe { Node::<ownership::Mut, height::TwoOrMore, T>::new(node) };
                    let (new_index, child_index) = internal
                        .node()
                        .lengths
                        .child_containing_index(index);
                    unsafe { internal.add_length_wrapping(child_index, 1_usize.wrapping_neg()) };
                    let child = unsafe { internal.node_mut().children[child_index].assume_init() };
                    let ret = unsafe { remove_from_node(child, new_index, h - 1) };

                    internal.maybe_handle_underfull_child(child_index);
                    ret
                }
            }
        }

        let root = unsafe { self.root.assume_init() };
        let h = self.height;
        let ret = unsafe { remove_from_node(root, index, h) };

        if h > 0 {
            unsafe {
                let mut old_root = Internal::new(self.root.assume_init());

                if old_root.is_singleton() {
                    let new_root = old_root.children().remove(0);
                    old_root.free();
                    self.root.write(new_root);
                    debug_assert_ne!(self.height, 0);
                    self.height -= 1;
                }
            }
        } else if unsafe { LeafMut::new(root).len() == 0 } {
            unsafe { Leaf::new(root).free() };
        }

        ret
    }

    #[must_use]
    pub fn iter(&self) -> Iter<T> {
        unsafe { Iter::new(self, 0, self.len()) }
    }

    pub fn drain<R>(&mut self, range: R) -> Drain<T>
    where
        R: RangeBounds<usize>,
    {
        Drain::new(self, range)
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

impl<T: Hash> Hash for BVec<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        self.iter().for_each(|elem| elem.hash(state));
    }
}
/*
impl<T> Extend<T> for BVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let mut cursor = CursorInner::new_past_the_end(self);
        for v in iter {
            cursor.insert(v);
            cursor.leaf_index += 1;
        }
    }
}
*/
impl<T> Index<usize> for BVec<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
            .unwrap_or_else(|| panic_out_of_bounds(index, self.len()))
    }
}

impl<T> IndexMut<usize> for BVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let len = self.len();
        self.get_mut(index)
            .unwrap_or_else(|| panic_out_of_bounds(index, len))
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::hash::Hasher;

    use super::*;

    fn _assert_cursor_mut_lifetime_covariant<'a, 'b>(x: CursorMut<'a, i32>) -> CursorMut<'b, i32>
    where
        'a: 'b,
    {
        x
    }

    fn _assert_cursor_lifetime_covariant<'a, 'b>(x: Cursor<'a, i32>) -> Cursor<'b, i32>
    where
        'a: 'b,
    {
        x
    }

    fn _assert_cursor_value_covariant<'a, 'b, 'c>(x: Cursor<'a, &'b str>) -> Cursor<'a, &'c str>
    where
        'b: 'c,
    {
        x
    }

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

        assert_eq!(
            size_of::<BVec<i32>>(),
            size_of::<*mut ()>() + 2 * size_of::<usize>()
        )
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
            
            //for (a, b) in b.iter().zip(-500..) {
            //    assert_eq!(*a, b);
            //}
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
        let mut b = BVec::<i32>::new();

        for x in 0..1000 {
            let index = rng.gen_range(0..=v.len());
            v.insert(index, x);
            b.insert(index, x);
            assert_eq!(v.len(), b.len());
        }

        //assert_eq!(v, b_4_5.iter().copied().collect::<Vec<_>>());
        //assert_eq!(v, b_5_4.iter().copied().collect::<Vec<_>>());
    }

    #[test]
    fn test_random_removals() {
        use alloc::vec::Vec;
        use rand::{Rng, SeedableRng};
        
        let mut rng = rand::rngs::StdRng::from_seed([123; 32]);
        
        let mut v = Vec::new();
        let mut b = BVec::<i32>::new();
        
        for x in 0..1000 {
            v.push(x);
            b.push_back(x);
        }
        
        while !v.is_empty() {
            let index = rng.gen_range(0..v.len());
            let v_rem = v.remove(index);
            let b_rem = b.remove(index);
            assert_eq!(v.len(), b.len());
            assert_eq!(v_rem, b_rem);
        }
        
        assert!(b.is_empty());
    }
    /*

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
        let n = 1000;
        let mut b = BVec::new();
        for x in 0..n {
            b.push_back(x);
        }

        assert!(b.iter().copied().eq(0..n));
    }

    #[test]
    fn test_bvec_extend() {
        let n = 500;
        let mut b = BVec::new();
        for x in 0..n {
            b.push_back(x);
        }

        b.extend(n..2 * n);

        assert!(b.iter().copied().eq(0..2 * n));
    }

    #[test]
    fn test_bvec_move_empty_cursor() {
        let mut b = BVec::<i32>::new();
        let mut c = b.cursor_at_mut(0);
        c.move_(0);
    }

    #[test]
    fn test_bvec_hash() {
        let n = 1000;
        let mut b = BVec::new();
        b.extend(0..n);

        let v = alloc::vec::Vec::from_iter(0..n);

        let mut v_hasher = std::collections::hash_map::DefaultHasher::new();
        v.hash(&mut v_hasher);
        let v_hash = v_hasher.finish();

        let mut b_hasher = std::collections::hash_map::DefaultHasher::new();
        b.hash(&mut b_hasher);
        let b_hash = b_hasher.finish();

        assert_eq!(v_hash, b_hash);
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
    }*/
}
