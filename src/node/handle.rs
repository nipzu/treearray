use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::{ptr, slice};

use alloc::boxed::Box;

use super::{DynNode, DynNodeMut, Node};

use crate::utils::slice_insert_forget_last;

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
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.ptr` union can be read.
            let values_ptr = self.node.ptr.values.as_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self.node`'s and the slice is thus not going to be written to during
            // the lifetime. `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts(values_ptr.cast(), self.len())
        }
    }
}

pub struct LeafMut<'a, T, const B: usize, const C: usize> {
    pub node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> LeafMut<'a, T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.node.length = NonZeroUsize::new(new_len).unwrap();
    }

    pub unsafe fn pop_back(&mut self) -> T {
        unsafe {
            self.set_len(self.len() - 1);
            self.node.ptr.values.as_ref()[self.len()].as_ptr().read()
        }
    }

    pub unsafe fn pop_front(&mut self) -> T {
        unsafe {
            self.set_len(self.len() - 1);
            let ret = self.node.ptr.values.as_mut()[0].as_ptr().read();
            let new_len = self.len();
            let value_ptr = self.node.ptr.values.as_mut().as_mut_ptr();
            ptr::copy(value_ptr.add(1), value_ptr, new_len);
            ret
        }
    }

    pub fn values(&self) -> &[T] {
        debug_assert!(self.len() <= C);
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.ptr` union can be read.
            let values_ptr = self.node.ptr.values.as_ref().as_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self`'s and the returned reference has thus unique access.
            // `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts(values_ptr.cast(), self.len())
        }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        debug_assert!(self.len() <= C);
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.ptr` union can be read.
            let values_ptr = self.node.ptr.values.as_mut().as_mut_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self`'s and the returned reference has thus unique access.
            // `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts_mut(values_ptr.cast(), self.len())
        }
    }

    pub fn into_values_mut(self) -> &'a mut [T] {
        debug_assert!(self.len() <= C);
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.ptr` union can be read.
            let values_ptr = self.node.ptr.values.as_mut().as_mut_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self.node`'s and the returned reference has thus unique access.
            // `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts_mut(values_ptr.cast(), self.len())
        }
    }

    fn is_full(&self) -> bool {
        self.values().len() == C
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Node<T, B, C>> {
        if self.is_full() {
            return Some(self.split_and_insert_value(index, value));
        }

        self.insert_fitting_extending(index, value);
        None
    }

    fn insert_fitting_extending(&mut self, index: usize, value: T) {
        assert!(self.len() < C);
        assert!(index <= self.len());
        unsafe {
            let index_ptr = self.node.ptr.values.as_mut().as_mut_ptr().add(index);
            ptr::copy(index_ptr, index_ptr.add(1), self.len() - index);
            ptr::write(index_ptr, MaybeUninit::new(value));
            self.set_len(self.len() + 1);
        }
    }

    fn split_and_insert_value(&mut self, index: usize, value: T) -> Node<T, B, C> {
        assert!(index <= self.len());

        unsafe {
            if index <= C / 2 {
                self.split_and_insert_left(index, value)
            } else {
                self.split_and_insert_right(index, value)
            }
        }
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::UNINIT; C]);
        let split_index = C / 2;
        let tail_len = C - split_index;

        unsafe {
            let values_ptr = self.values_mut().as_mut_ptr();
            let index_ptr = values_ptr.add(index);
            let split_ptr = values_ptr.add(split_index);
            let box_ptr = new_box.as_mut_ptr();
            ptr::copy_nonoverlapping(split_ptr, box_ptr.cast::<T>(), tail_len);
            ptr::copy(index_ptr, index_ptr.add(1), split_index - index);
            ptr::write(index_ptr, value);

            self.set_len(split_index + 1);
            Node::from_values(tail_len, new_box)
        }
    }

    unsafe fn split_and_insert_right(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::UNINIT; C]);
        let split_index = C / 2 + 1;
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
            let index_ptr = self.values_mut().as_mut_ptr().add(index);
            let ret = index_ptr.read();
            ptr::copy(index_ptr.add(1), index_ptr, self.len() - index - 1);
            self.set_len(self.len() - 1);
            ret
        }
    }
}

pub struct Internal<'a, T, const B: usize, const C: usize> {
    height: NonZeroUsize,
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> Internal<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(height: usize, node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() > C);
        Self {
            height: NonZeroUsize::new(height).unwrap(),
            node,
        }
    }

    // pub fn children(&self) -> &'a [Option<Node<T, B, C>>; B] {
    //     debug_assert!(self.node.len() > C);
    //     // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
    //     // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
    //     unsafe { &self.node.ptr.children }
    // }

    pub fn children<'b>(&'b self) -> impl Iterator<Item = DynNode<'a, T, B, C>> + 'b {
        debug_assert!(self.node.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        let children = unsafe { self.node.ptr.children.as_ref() };
        let child_height = self.height.get() - 1;
        children
            .iter()
            .map_while(move |m| m.as_ref().map(|n| unsafe { DynNode::new(child_height, n) }))
    }
}

pub struct InternalMut<'a, T, const B: usize, const C: usize> {
    height: NonZeroUsize,
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, T, B, C> {
    const NONE: Option<Node<T, B, C>> = None;

    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(height: usize, node: &'a mut Node<T, B, C>) -> Self {
        // debug_assert!(node.len() > C);
        Self {
            height: NonZeroUsize::new(height).unwrap(),
            node,
        }
    }

    fn is_full(&self) -> bool {
        matches!(self.children().last(), Some(&Some(_)))
    }

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub fn set_len(&mut self, new_len: usize) {
        // debug_assert!(new_len > C);
        self.node.length = NonZeroUsize::new(new_len).unwrap();
    }

    pub fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        // debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub fn children_mut(&mut self) -> impl Iterator<Item = DynNodeMut<T, B, C>> + '_ {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        let children = unsafe { self.node.ptr.children.as_mut() };
        let child_height = self.height.get() - 1;
        children.iter_mut().map_while(move |m| {
            m.as_mut()
                .map(|n| unsafe { DynNodeMut::new(child_height, n) })
        })
    }

    pub fn children_slice_mut(&mut self) -> &mut [Option<Node<T, B, C>>] {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn into_children_mut(self) -> impl Iterator<Item = DynNodeMut<'a, T, B, C>> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        let children = unsafe { self.node.ptr.children.as_mut() };
        let child_height = self.height.get() - 1;
        children.iter_mut().map_while(move |m| {
            m.as_mut()
                .map(|n| unsafe { DynNodeMut::new(child_height, n) })
        })
    }

    pub unsafe fn insert_node(
        &mut self,
        index: usize,
        new_child: Node<T, B, C>,
    ) -> Option<Node<T, B, C>> {
        if self.is_full() {
            unsafe {
                return Some(self.split_and_insert_node(index + 1, new_child));
            }
        }
        self.insert_fitting(index + 1, new_child);
        self.set_len(self.len() + 1);
        None
    }

    fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        slice_insert_forget_last(self.children_slice_mut(), index, Some(node));
    }

    unsafe fn split_and_insert_node(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        unsafe {
            if index <= B / 2 {
                self.split_and_insert_left(index, node)
            } else {
                self.split_and_insert_right(index, node)
            }
        }
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);
        let node_len = node.len();
        let split_index = B / 2;
        let tail_len = B - split_index;

        let new_self_len = sum_lens(&self.children_slice_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_slice_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

        slice_insert_forget_last(
            &mut self.children_slice_mut()[..=split_index],
            index,
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
        let split_index = B / 2 + 1;
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
}

fn sum_lens<T, const B: usize, const C: usize>(children: &[Option<Node<T, B, C>>]) -> usize {
    children
        .iter()
        .map(|n| n.as_ref().map_or(0, Node::len))
        .sum()
}
