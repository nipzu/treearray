use core::{
    hint::unreachable_unchecked, mem::MaybeUninit, num::NonZeroUsize, ops::RangeBounds, ptr, slice,
};

use alloc::boxed::Box;

use crate::{
    node::Node,
    utils::{slice_assume_init_mut, slice_shift_left, slice_shift_right},
};

pub struct Leaf<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Leaf<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub unsafe fn value_unchecked(&self, index: usize) -> &'a T {
        debug_assert!(self.len() <= C);
        debug_assert!(index < self.len());

        // We own a shared reference to this leaf, so there
        // should not be any mutable references which
        // could cause aliasing problems with taking
        // a reference to the whole array.
        unsafe {
            self.node
                .ptr
                .values
                .as_ref()
                .get_unchecked(index)
                .assume_init_ref()
        }
    }
}

pub struct LeafMut<'a, T, const B: usize, const C: usize> {
    pub node: &'a mut Option<Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new(node: &'a mut Option<Node<T, B, C>>) -> Self {
        debug_assert!(node.as_ref().unwrap().len() <= C);
        debug_assert!(node.as_ref().unwrap().len() != 0);
        Self { node }
    }

    pub fn len(&self) -> usize {
        self.node().len()
    }

    fn node(&self) -> &Node<T, B, C> {
        self.node.as_ref().unwrap()
    }

    fn node_mut(&mut self) -> &mut Node<T, B, C> {
        self.node.as_mut().unwrap()
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= C);
        let new_len = NonZeroUsize::new(new_len).unwrap();
        self.node_mut().length = new_len;
    }

    pub fn free(mut self) {
        unsafe { Box::from_raw(self.values_maybe_uninit_mut()) };
        *self.node = None;
    }

    pub unsafe fn pop_back(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe { self.remove_unchecked(self.len() - 1) }
    }

    pub unsafe fn pop_front(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe { self.remove_unchecked(0) }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        let len = self.len();
        debug_assert!(len <= C);
        unsafe { slice_assume_init_mut(self.values_maybe_uninit_mut().get_unchecked_mut(..len)) }
    }

    pub fn values_maybe_uninit_mut(&mut self) -> &mut [MaybeUninit<T>; C] {
        unsafe { self.node_mut().ptr.values.as_mut() }
    }

    pub unsafe fn into_value_unchecked_mut(mut self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= C);
        debug_assert!(index < len);
        unsafe {
            self.node_mut()
                .ptr
                .values
                .as_mut()
                .get_unchecked_mut(index)
                .assume_init_mut()
        }
    }

    fn is_full(&self) -> bool {
        self.len() == C
    }

    pub fn is_almost_underfull(&self) -> bool {
        self.len() == (C - 1) / 2 + 1
    }

    pub fn insert_value(&mut self, index: usize, value: T) -> InsertResult<T, B, C> {
        assert!(index <= self.len());

        unsafe {
            if self.is_full() {
                InsertResult::Split(if index <= C / 2 {
                    SplitResult::Left(self.split_and_insert_left(index, value))
                } else {
                    SplitResult::Right(self.split_and_insert_right(index, value))
                })
            } else {
                self.insert_fitting(index, value);
                InsertResult::Fit
            }
        }
    }

    fn insert_fitting(&mut self, index: usize, value: T) {
        assert!(self.len() < C);
        unsafe {
            let old_len = self.len();
            slice_shift_right(
                &mut self.values_maybe_uninit_mut()[index..=old_len],
                MaybeUninit::new(value),
            );
            self.set_len(old_len + 1);
        }
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::UNINIT; C]);
        let split_index = C / 2;
        let tail_len = C - split_index;

        unsafe {
            let split_ptr = self.values_mut().as_mut_ptr().add(split_index);
            let box_ptr = new_box.as_mut_ptr();
            ptr::copy_nonoverlapping(split_ptr, box_ptr.cast::<T>(), tail_len);
            slice_shift_right(
                &mut self.values_maybe_uninit_mut()[index..=split_index],
                MaybeUninit::new(value),
            );

            self.set_len(split_index + 1);
            Node::from_values(tail_len, new_box)
        }
    }

    unsafe fn split_and_insert_right(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::UNINIT; C]);
        let split_index = (C - 1) / 2 + 1;
        let tail_len = C - split_index;

        let tail_start_len = index - split_index;
        let tail_end_len = tail_len - tail_start_len;

        unsafe {
            let split_ptr = self.values_mut().as_mut_ptr().add(split_index);
            let box_ptr = new_box.as_mut_ptr();
            ptr::copy_nonoverlapping(split_ptr, box_ptr.cast::<T>(), tail_start_len);
            ptr::write(box_ptr.add(tail_start_len).cast::<T>(), value);
            ptr::copy_nonoverlapping(
                split_ptr.add(tail_start_len),
                box_ptr.cast::<T>().add(tail_start_len + 1),
                tail_end_len,
            );

            self.set_len(split_index);
            Node::from_values(tail_len + 1, new_box)
        }
    }

    pub unsafe fn remove_unchecked(&mut self, index: usize) -> T {
        debug_assert!(index < self.len());

        unsafe {
            let old_len = self.len();
            self.set_len(old_len - 1);
            let slice = self
                .values_maybe_uninit_mut()
                .get_unchecked_mut(index..old_len);
            slice_shift_left(slice, MaybeUninit::uninit()).assume_init()
        }
    }
}

pub struct Internal<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Internal<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        Self { node }
    }

    pub fn is_singleton(&self) -> bool {
        self.children()[1].is_none()
    }

    pub fn children(&self) -> &'a [Option<Node<T, B, C>>; B] {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> &'a Node<T, B, C> {
        fn child_len<T, const B: usize, const C: usize>(child: &Option<Node<T, B, C>>) -> usize {
            child.as_ref().map_or(0, Node::len)
        }

        for child in self.children() {
            let len = child_len(child);
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return unsafe { child.as_ref().unwrap_unchecked() },
            }
        }

        unsafe { unreachable_unchecked() };
    }
}

pub struct InternalMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Option<Node<T, B, C>>,
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, T, B, C> {
    const NONE: Option<Node<T, B, C>> = None;

    pub const UNDERFULL_LEN: usize = (B - 1) / 2;

    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: &'a mut Option<Node<T, B, C>>) -> Self {
        Self { node }
    }

    pub unsafe fn from_ptr(ptr: *mut Option<Node<T, B, C>>) -> Self {
        unsafe { Self::new(&mut *ptr) }
    }

    fn node(&self) -> &Node<T, B, C> {
        self.node.as_ref().unwrap()
    }

    fn node_mut(&mut self) -> &mut Node<T, B, C> {
        self.node.as_mut().unwrap()
    }

    fn is_full(&self) -> bool {
        matches!(self.children().last(), Some(&Some(_)))
    }

    pub fn is_underfull(&self) -> bool {
        self.children()[Self::UNDERFULL_LEN].is_none()
    }

    pub fn is_almost_underfull(&self) -> bool {
        self.children()[Self::UNDERFULL_LEN + 1].is_none()
    }

    pub fn len(&self) -> usize {
        self.node().len()
    }

    pub fn set_len(&mut self, new_len: usize) {
        let new_len = NonZeroUsize::new(new_len).unwrap();
        self.node_mut().length = new_len;
    }

    pub unsafe fn into_child_containing_index(
        self,
        index: &mut usize,
    ) -> &'a mut Option<Node<T, B, C>> {
        fn child_len<T, const B: usize, const C: usize>(child: &Option<Node<T, B, C>>) -> usize {
            child.as_ref().map_or(0, Node::len)
        }

        for child in self.into_children_mut() {
            let len = child_len(child);
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, elem_ptr: *const Option<Node<T, B, C>>) -> usize {
        let slice_ptr = unsafe { self.node().ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            elem_ptr.offset_from(slice_ptr.cast()) as usize
        }
    }

    pub fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node().ptr.children.as_ref() }
    }

    pub fn children_mut(&mut self) -> &mut [Option<Node<T, B, C>>; B] {
        unsafe { self.node_mut().ptr.children.as_mut() }
    }

    pub fn into_children_mut(mut self) -> &'a mut [Option<Node<T, B, C>>; B] {
        unsafe { self.node_mut().ptr.children.as_mut() }
    }

    pub fn free(mut self) {
        debug_assert!(self.children().iter().all(Option::is_none));
        unsafe { Box::from_raw(self.children_mut()) };
        *self.node = None;
    }

    pub unsafe fn children_range_mut(
        &mut self,
        range: impl RangeBounds<usize>,
    ) -> &mut [Option<Node<T, B, C>>] {
        let start = match range.start_bound() {
            core::ops::Bound::Included(i) => *i,
            core::ops::Bound::Excluded(i) => i + 1,
            core::ops::Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            core::ops::Bound::Included(i) => i + 1,
            core::ops::Bound::Excluded(i) => *i,
            core::ops::Bound::Unbounded => B,
        };

        unsafe {
            slice::from_raw_parts_mut(
                self.node
                    .as_mut()
                    .unwrap()
                    .ptr
                    .children
                    .as_ptr()
                    .cast::<Option<Node<T, B, C>>>()
                    .add(start),
                end - start,
            )
        }
    }

    pub fn child_mut(&mut self, index: usize) -> &mut Option<Node<T, B, C>> {
        unsafe { &mut (*self.node_mut().ptr.children.as_ptr())[index] }
    }

    pub unsafe fn insert_node(
        &mut self,
        index: usize,
        prev_split_res: SplitResult<T, B, C>,
    ) -> InsertResult<T, B, C> {
        let (path_through_self, node) = match prev_split_res {
            SplitResult::Right(n) => (false, n),
            SplitResult::Left(n) => (true, n),
        };
        unsafe {
            if self.is_full() {
                use core::cmp::Ordering::{Equal, Less};
                InsertResult::Split(match index.cmp(&(Self::UNDERFULL_LEN + 1)) {
                    Less => SplitResult::Left(self.split_and_insert_left(index, node)),
                    Equal if path_through_self => {
                        SplitResult::Left(self.split_and_insert_right(index, node))
                    }
                    _ => SplitResult::Right(self.split_and_insert_right(index, node)),
                })
            } else {
                self.insert_fitting(index, node);
                InsertResult::Fit
            }
        }
    }

    fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        slice_shift_right(&mut self.children_mut()[index..], Some(node));
        self.set_len(self.len() + 1);
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);
        let node_len = node.len();
        let split_index = Self::UNDERFULL_LEN;
        let tail_len = B - split_index;

        let new_self_len = sum_lens(&self.children_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

        slice_shift_right(&mut self.children_mut()[index..=split_index], Some(node));

        debug_assert_eq!(new_self_len + node_len, sum_lens(self.children()));
        debug_assert_eq!(new_nodes_len + 1, sum_lens(new_box.as_ref()));
        self.set_len(new_self_len + node_len);
        Node::from_children(new_nodes_len + 1, new_box)
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);
        let node_len = node.len();
        let split_index = Self::UNDERFULL_LEN + 1;
        let tail_len = B - split_index;

        let tail_start_len = index - split_index;

        let new_self_len = sum_lens(&self.children_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_mut()[split_index..index].swap_with_slice(&mut new_box[..tail_start_len]);
        self.children_mut()[index..].swap_with_slice(&mut new_box[tail_start_len + 1..=tail_len]);
        new_box[tail_start_len] = Some(node);

        debug_assert_eq!(new_self_len, sum_lens(self.children()));
        debug_assert_eq!(new_nodes_len + node_len + 1, sum_lens(new_box.as_ref()));
        self.set_len(new_self_len);
        Node::from_children(new_nodes_len + node_len + 1, new_box)
    }

    pub fn reborrow(&mut self) -> InternalMut<'_, T, B, C> {
        InternalMut { node: self.node }
    }

    pub unsafe fn append_from(
        &mut self,
        mut other: InternalMut<T, B, C>,
        self_len: usize,
        other_len: usize,
    ) {
        unsafe {
            self.children_range_mut(self_len..self_len + other_len)
                .swap_with_slice(&mut other.children_mut()[..other_len]);
        }
        self.set_len(self.len() + other.len());
        debug_assert!(other.children().iter().all(Option::is_none));
        other.free();
    }

    pub unsafe fn rotate_from_next(&mut self, mut next: InternalMut<T, B, C>) {
        let x = slice_shift_left(next.children_mut(), None).unwrap();

        next.set_len(next.len() - x.len());
        self.set_len(self.len() + x.len());

        *self.child_mut(InternalMut::<T, B, C>::UNDERFULL_LEN) = Some(x);
    }

    pub unsafe fn rotate_from_previous(&mut self, mut prev: InternalMut<T, B, C>) {
        for i in (0..B).rev() {
            if let Some(x) = prev.child_mut(i).take() {
                prev.set_len(prev.len() - x.len());
                self.set_len(self.len() + x.len());

                slice_shift_right(self.children_mut(), Some(x));
                return;
            }
        }
        unreachable!();
    }
}

fn sum_lens<T, const B: usize, const C: usize>(children: &[Option<Node<T, B, C>>]) -> usize {
    children
        .iter()
        .map(|n| n.as_ref().map_or(0, Node::len))
        .sum()
}

pub enum InsertResult<T, const B: usize, const C: usize> {
    Fit,
    Split(SplitResult<T, B, C>),
}

pub enum SplitResult<T, const B: usize, const C: usize> {
    Left(Node<T, B, C>),
    Right(Node<T, B, C>),
}
