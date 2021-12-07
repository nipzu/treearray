use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::{ptr, slice};

use alloc::boxed::Box;

use super::Node;

pub struct LeafHandle<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> LeafHandle<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    const fn len(&self) -> usize {
        self.node.len()
    }

    pub fn values(&self) -> &'a [T] {
        debug_assert!(self.len() <= C);
        // TODO: feature(maybe_uninit_slice) https://github.com/rust-lang/rust/issues/63569
        // MaybeUninit::slice_assume_init_ref(&self.node.inner.values[..self.len()])

        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.inner` union can be read.
            let values_ptr = self.node.inner.values.as_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self.node`'s and the slice is thus not going to be written to during
            // the lifetime. `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts(values_ptr.cast(), self.len())
        }
    }
}

impl<'a, T, const B: usize, const C: usize> InternalHandle<'a, T, B, C> {
    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() > C);
        Self { node }
    }

    pub fn children(&self) -> &'a [Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.inner` union can be read.
        unsafe { &self.node.inner.children }
    }
}

pub struct LeafHandleMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> LeafHandleMut<'a, T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    /// # Safety:
    ///
    /// `node` must be a leaf node i.e. `node.len() <= C`.
    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    const fn len(&self) -> usize {
        self.node.len()
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        debug_assert!(self.len() <= C);
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.inner` union can be read.
            let values_ptr = (*self.node.inner.values).as_mut_ptr();
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
            // `Self::new`, so the `values` field of the `self.node.inner` union can be read.
            let values_ptr = (*self.node.inner.values).as_mut_ptr();
            // SAFETY: According to the invariants of `Node`, at least `self.len()`
            // values are guaranteed to be initialized and valid for use. The lifetime is the
            // same as `self.node`'s and the returned reference has thus unique access.
            // `self.len() * size_of::<T>()` is no greater than `isize::MAX`
            // by the const invariants of `BTreeVec`.
            slice::from_raw_parts_mut(values_ptr.cast(), self.len())
        }
    }

    fn is_full(&self) -> bool {
        self.node.is_full()
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Node<T, B, C>> {
        if self.is_full() {
            return Some(self.split_and_insert_value(index, value));
        }

        unsafe { self.insert_fitting_extending(index, value) };
        None
    }

    unsafe fn insert_fitting_extending(&mut self, index: usize, value: T) {
        debug_assert!(self.len() < C);
        debug_assert!(index <= self.len());
        unsafe {
            let index_ptr = (*self.node.inner.values).as_mut_ptr().add(index);
            ptr::copy(index_ptr, index_ptr.add(1), self.values_mut().len() - index);
            ptr::write(index_ptr, MaybeUninit::new(value));
            self.node.length = NonZeroUsize::new(self.len() + 1).unwrap();
        }
    }

    fn split_and_insert_value(&mut self, index: usize, value: T) -> Node<T, B, C> {
        assert!(index <= self.len());

        let mut new_box = Box::new([Self::UNINIT; C]);

        if index <= C / 2 {
            // insert to left
            let split_index = C / 2;
            let tail_len = C - split_index;

            unsafe {
                let index_ptr = self.values_mut().as_mut_ptr().add(index);
                let split_ptr = self.values_mut().as_mut_ptr().add(split_index);
                let box_ptr = new_box.as_mut_ptr();
                ptr::copy_nonoverlapping(split_ptr, box_ptr.cast::<T>(), tail_len);
                ptr::copy(index_ptr, index_ptr.add(1), split_index - index);
                ptr::write(index_ptr, value);

                self.node.length = NonZeroUsize::new(split_index + 1).unwrap();
                Node::from_values(tail_len, new_box)
            }
        } else {
            // insert to right
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

                self.node.length = NonZeroUsize::new(split_index).unwrap();
                Node::from_values(tail_len + 1, new_box)
            }
        }
    }
}

pub struct InternalHandle<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

pub struct InternalHandleMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> InternalHandleMut<'a, T, B, C> {
    const NONE: Option<Node<T, B, C>> = None;

    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        debug_assert!(node.len() > C);
        Self { node }
    }

    fn is_full(&self) -> bool {
        self.node.is_full()
    }

    const fn len(&self) -> usize {
        self.node.len()
    }

    fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.inner` union can be read.
        unsafe { &self.node.inner.children }
    }

    pub fn children_mut(&mut self) -> &mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.inner` union can be read.
        unsafe { &mut self.node.inner.children }
    }

    pub fn into_children_mut(self) -> &'a mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.inner` union can be read.
        unsafe { &mut self.node.inner.children }
    }

    fn find_insert_index(&mut self, mut index: usize) -> (usize, usize) {
        for (i, maybe_child) in self.children().iter().enumerate() {
            if let Some(child) = maybe_child {
                if index <= child.len() {
                    return (i, index);
                }
                index -= child.len();
            }
        }
        unreachable!();
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Node<T, B, C>> {
        let (insert_index, child_index) = self.find_insert_index(index);

        if let Some(new_child) = self.children_mut()[insert_index]
            .as_mut()
            .and_then(|n| n.insert(child_index, value))
        {
            unsafe {
                if self.is_full() {
                    return Some(self.split_and_insert_node(insert_index + 1, new_child));
                }

                self.insert_fitting(insert_index + 1, new_child);
            }
        }
        self.node.length = NonZeroUsize::new(self.len() + 1).unwrap();
        None
    }

    unsafe fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        unsafe {
            slice_insert_forget_last(self.children_mut(), index, Some(node));
        }
    }

    unsafe fn split_and_insert_node(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);
        let node_len = node.len();

        if index <= B / 2 {
            // insert to left
            let split_index = B / 2;
            let tail_len = B - split_index;

            let new_self_len = sum_lens(&self.children_mut()[..split_index]);
            let new_nodes_len = self.len() - node_len - new_self_len;

            self.children_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

            unsafe {
                slice_insert_forget_last(
                    &mut self.children_mut()[..=split_index],
                    index,
                    Some(node),
                );
            }

            self.node.length = NonZeroUsize::new(new_self_len + node_len).unwrap();
            debug_assert_eq!(new_self_len + node_len, sum_lens(self.children()));
            debug_assert_eq!(new_nodes_len + 1, sum_lens(new_box.as_ref()));
            Node::from_children(new_nodes_len + 1, new_box)
        } else {
            // insert to right
            let split_index = B / 2 + 1;
            let tail_len = B - split_index;

            let tail_start_len = index - split_index;

            let new_self_len = sum_lens(&self.children_mut()[..split_index]);
            let new_nodes_len = self.len() - node_len - new_self_len;

            self.children_mut()[split_index..index].swap_with_slice(&mut new_box[..tail_start_len]);
            self.children_mut()[index..]
                .swap_with_slice(&mut new_box[tail_start_len + 1..=tail_len]);
            new_box[tail_start_len] = Some(node);

            assert!(new_self_len > C);
            self.node.length = NonZeroUsize::new(new_self_len).unwrap();
            debug_assert_eq!(new_self_len, sum_lens(self.children()));
            debug_assert_eq!(new_nodes_len + node_len + 1, sum_lens(new_box.as_ref()));
            Node::from_children(new_nodes_len + node_len + 1, new_box)
        }
    }
}

unsafe fn slice_insert_forget_last<T>(slice: &mut [T], index: usize, value: T) {
    debug_assert!(!slice.is_empty());
    debug_assert!(index < slice.len());
    unsafe {
        let index_ptr = slice.as_mut_ptr().add(index);
        ptr::copy(index_ptr, index_ptr.add(1), slice.len() - index - 1);
        ptr::write(index_ptr, value);
    }
}

fn sum_lens<T, const B: usize, const C: usize>(children: &[Option<Node<T, B, C>>]) -> usize {
    children
        .iter()
        .map(|n| n.as_ref().map_or(0, Node::len))
        .sum()
}
