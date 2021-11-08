use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::{ptr, slice};

use alloc::boxed::Box;

use super::Node;

pub struct LeafHandle<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

pub struct LeafHandleMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

pub struct InternalHandle<'a, T, const B: usize, const C: usize> {
    node: &'a Node<T, B, C>,
}

pub struct InternalHandleMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> LeafHandle<'a, T, B, C> {
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    pub fn values(&self) -> &'a [T] {
        debug_assert!(self.node.len() <= C);
        unsafe {
            let values_ptr = self.node.inner.values.as_ptr().cast();
            slice::from_raw_parts(values_ptr, self.node.len())
        }
    }
}

impl<'a, T, const B: usize, const C: usize> LeafHandleMut<'a, T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        debug_assert!(node.len() <= C);
        Self { node }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        debug_assert!(self.node.len() <= C);
        unsafe {
            let values_ptr = (*self.node.inner.values).as_mut_ptr().cast::<T>();
            slice::from_raw_parts_mut(values_ptr, self.node.len())
        }
    }

    pub fn into_values_mut(self) -> &'a mut [T] {
        debug_assert!(self.node.len() <= C);
        unsafe {
            let values_ptr = (*self.node.inner.values).as_mut_ptr().cast::<T>();
            slice::from_raw_parts_mut(values_ptr, self.node.len())
        }
    }

    pub fn is_full(&self) -> bool {
        self.node.is_full()
    }

    pub unsafe fn insert_fitting(&mut self, index: usize, value: T) {
        debug_assert!(self.node.len() < C);
        debug_assert!(index <= self.node.len());
        unsafe {
            let index_ptr = (*self.node.inner.values).as_mut_ptr().add(index);
            ptr::copy(index_ptr, index_ptr.add(1), self.values_mut().len() - index);
            ptr::write(index_ptr, MaybeUninit::new(value));
        }
    }

    pub unsafe fn split_and_insert_value(&mut self, index: usize, value: T) -> Node<T, B, C> {
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

impl<'a, T, const B: usize, const C: usize> InternalHandle<'a, T, B, C> {
    pub const unsafe fn new(node: &'a Node<T, B, C>) -> Self {
        debug_assert!(node.len() > C);
        Self { node }
    }

    pub fn children(&self) -> &'a [Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
        unsafe { &self.node.inner.children }
    }
}

impl<'a, T, const B: usize, const C: usize> InternalHandleMut<'a, T, B, C> {
    const NONE: Option<Node<T, B, C>> = None;

    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        debug_assert!(node.len() > C);
        Self { node }
    }

    pub fn is_full(&self) -> bool {
        self.node.is_full()
    }

    pub fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
        unsafe { &self.node.inner.children }
    }

    pub fn children_mut(&mut self) -> &mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
        unsafe { &mut self.node.inner.children }
    }

    pub fn into_children_mut(self) -> &'a mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
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

    pub fn insert(&mut self, index: usize, value: T) -> Option<(Node<T, B, C>, usize)> {
        let (insert_index, child_index) = self.find_insert_index(index);
        self.children_mut()[insert_index]
            .as_mut()
            .and_then(|n| n.insert(child_index, value).map(|n| (n, insert_index)))
    }

    pub unsafe fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        unsafe {
            slice_insert_forget_last(self.children_mut(), index, Some(node));
        }
    }

    pub unsafe fn split_and_insert_node(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        let mut new_box = Box::new([Self::NONE; B]);

        if index <= B / 2 {
            // insert to left
            let split_index = B / 2;
            let tail_len = B - split_index;

            self.children_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

            unsafe {
                slice_insert_forget_last(
                    &mut self.children_mut()[..=split_index],
                    index,
                    Some(node),
                );
            }

            self.node.length = NonZeroUsize::new(split_index + 1).unwrap();
            Node::from_children(tail_len, new_box)
        } else {
            // insert to right
            let split_index = B / 2 + 1;
            let tail_len = B - split_index;

            let tail_start_len = index - split_index;

            self.children_mut()[split_index..index].swap_with_slice(&mut new_box[..tail_start_len]);
            self.children_mut()[index..].swap_with_slice(&mut new_box[tail_start_len + 1..=tail_len]);
            new_box[tail_start_len] = Some(node);

            self.node.length = NonZeroUsize::new(split_index).unwrap();
            Node::from_children(tail_len + 1, new_box)
        }
    }
}

unsafe fn slice_insert_forget_last<T>(slice: &mut [T], index: usize, value: T) {
    debug_assert!(!slice.is_empty());
    debug_assert!(index <= slice.len());
    unsafe {
        let index_ptr = slice.as_mut_ptr().add(index);
        ptr::copy(index_ptr, index_ptr.add(1), slice.len() - index - 1);
        ptr::write(index_ptr, value);
    }
}
