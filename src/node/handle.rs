use core::{
    mem::{self, MaybeUninit},
    num::NonZeroUsize,
    ops::RangeBounds,
    ptr, slice,
};

use alloc::boxed::Box;

use crate::{
    node::Node,
    utils::{slice_assume_init_mut, slice_assume_init_ref, slice_shift_left, slice_shift_right},
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

    pub fn values(&self) -> &'a [T] {
        debug_assert!(self.len() <= C);
        unsafe { slice_assume_init_ref(&self.node.ptr.values.as_ref()[..self.len()]) }
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
        unsafe {
            Box::from_raw(self.values_maybe_uninit_mut());
        }
        *self.node = None;
    }

    pub unsafe fn pop_back(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe {
            self.set_len(self.len() - 1);
            self.values_maybe_uninit()[self.len()].as_ptr().read()
        }
    }

    pub unsafe fn pop_front(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe {
            let old_len = self.len();
            self.set_len(old_len - 1);
            slice_shift_left(
                &mut self.values_maybe_uninit_mut()[..old_len],
                MaybeUninit::uninit(),
            )
            .assume_init()
        }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        let len = self.len();
        debug_assert!(len <= C);
        unsafe { slice_assume_init_mut(&mut self.node_mut().ptr.values.as_mut()[..len]) }
    }

    pub fn values_maybe_uninit_mut(&mut self) -> &mut [MaybeUninit<T>; C] {
        unsafe { self.node_mut().ptr.values.as_mut() }
    }

    pub fn values_maybe_uninit(&self) -> &[MaybeUninit<T>; C] {
        unsafe { self.node().ptr.values.as_ref() }
    }

    pub fn into_values_mut(mut self) -> &'a mut [T] {
        let len = self.len();
        debug_assert!(len <= C);
        unsafe { slice_assume_init_mut(&mut self.node_mut().ptr.values.as_mut()[..len]) }
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
                if index <= C / 2 {
                    InsertResult::SplitLeft(self.split_and_insert_left(index, value))
                } else {
                    InsertResult::SplitRight(self.split_and_insert_right(index, value))
                }
            } else {
                self.insert_fitting_extending(index, value);
                InsertResult::Fit
            }
        }
    }

    fn insert_fitting_extending(&mut self, index: usize, value: T) {
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

    pub unsafe fn remove_no_underflow(&mut self, index: usize) -> T {
        debug_assert!(index < self.len());

        unsafe {
            let old_len = self.len();
            self.set_len(old_len - 1);
            slice_shift_left(
                &mut self.values_maybe_uninit_mut()[index..old_len],
                MaybeUninit::uninit(),
            )
            .assume_init()
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
        // TODO: this is potentially slow
        self.children().count() == 1
    }

    pub fn children(&self) -> impl Iterator<Item = &'a Node<T, B, C>> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        let children = unsafe { self.node.ptr.children.as_ref() };
        children.iter().map_while(Option::as_ref)
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

    pub fn index_of_child_ptr(&self, elem_ptr: *const Option<Node<T, B, C>>) -> usize {
        let slice_addr = unsafe { self.node().ptr.children.as_ptr() as usize };
        let elem_addr = elem_ptr as usize;
        (elem_addr - slice_addr) / mem::size_of::<Option<Node<T, B, C>>>()
    }

    pub fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node().ptr.children.as_ref() }
    }

    pub fn into_children_slice_mut(mut self) -> &'a mut [Option<Node<T, B, C>>; B] {
        unsafe { self.node_mut().ptr.children.as_mut() }
    }

    pub fn children_slice_mut(&mut self) -> &mut [Option<Node<T, B, C>>; B] {
        unsafe { self.node_mut().ptr.children.as_mut() }
    }

    pub fn free(mut self) {
        unsafe {
            Box::from_raw(self.children_slice_mut());
        }
        *self.node = None;
    }

    pub unsafe fn children_slice_range_mut(
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

    pub fn get_child_mut(&mut self, index: usize) -> &mut Option<Node<T, B, C>> {
        unsafe { &mut (*self.node_mut().ptr.children.as_ptr())[index] }
    }

    pub unsafe fn insert_node(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
        path_through_new: bool,
    ) -> InsertResult<T, B, C> {
        unsafe {
            if self.is_full() {
                use core::cmp::Ordering::{Equal, Greater, Less};
                match index.cmp(&(Self::UNDERFULL_LEN + 1)) {
                    Less => InsertResult::SplitLeft(self.split_and_insert_left(index, node)),
                    Greater => InsertResult::SplitRight(self.split_and_insert_right(index, node)),
                    Equal => {
                        let n = self.split_and_insert_right(index, node);
                        if path_through_new {
                            InsertResult::SplitRight(n)
                        } else {
                            InsertResult::SplitLeft(n)
                        }
                    }
                }
            } else {
                self.insert_fitting(index, node);
                InsertResult::Fit
            }
        }
    }

    fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        slice_shift_right(&mut self.children_slice_mut()[index..], Some(node));
        self.set_len(self.len() + 1);
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);
        let node_len = node.len();
        let split_index = Self::UNDERFULL_LEN;
        let tail_len = B - split_index;

        let new_self_len = sum_lens(&self.children_slice_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_slice_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

        slice_shift_right(
            &mut self.children_slice_mut()[index..=split_index],
            Some(node),
        );

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

        let new_self_len = sum_lens(&self.children_slice_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_slice_mut()[split_index..index]
            .swap_with_slice(&mut new_box[..tail_start_len]);
        self.children_slice_mut()[index..]
            .swap_with_slice(&mut new_box[tail_start_len + 1..=tail_len]);
        new_box[tail_start_len] = Some(node);

        debug_assert_eq!(new_self_len, sum_lens(self.children()));
        debug_assert_eq!(new_nodes_len + node_len + 1, sum_lens(new_box.as_ref()));
        self.set_len(new_self_len);
        Node::from_children(new_nodes_len + node_len + 1, new_box)
    }

    pub fn reborrow(&mut self) -> InternalMut<'_, T, B, C> {
        InternalMut { node: self.node }
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
    SplitLeft(Node<T, B, C>),
    SplitRight(Node<T, B, C>),
}
