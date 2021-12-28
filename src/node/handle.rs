use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZeroUsize;
use core::{mem, ptr, slice};

use alloc::boxed::Box;

use super::{Node, VariantMut};

use crate::utils::{slice_insert_forget_last, slice_shift_left, slice_shift_right};

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

    const fn len(&self) -> usize {
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
    node: &'a mut Node<T, B, C>,
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

    pub unsafe fn set_length(&mut self, new_length: NonZeroUsize) {
        self.node.length = new_length;
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.node.length = NonZeroUsize::new(new_len).unwrap();
    }

    unsafe fn pop_back(&mut self) -> T {
        unsafe {
            self.set_len(self.len() - 1);
            self.node.ptr.values[self.len()].as_ptr().read()
        }
    }

    unsafe fn pop_front(&mut self) -> T {
        unsafe {
            self.set_len(self.len() - 1);
            let ret = self.node.ptr.values[0].as_ptr().read();
            let new_len = self.len();
            let value_ptr = (*self.node.ptr.values).as_mut_ptr();
            ptr::copy(value_ptr.add(1), value_ptr, new_len);
            ret
        }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        debug_assert!(self.len() <= C);
        unsafe {
            // SAFETY: `self.node` is guaranteed to be a leaf node by the safety invariants of
            // `Self::new`, so the `values` field of the `self.node.ptr` union can be read.
            let values_ptr = (*self.node.ptr.values).as_mut_ptr();
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
            let values_ptr = (*self.node.ptr.values).as_mut_ptr();
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

        self.insert_fitting_extending(index, value);
        None
    }

    fn insert_fitting_extending(&mut self, index: usize, value: T) {
        assert!(self.len() < C);
        assert!(index <= self.len());
        unsafe {
            let index_ptr = (*self.node.ptr.values).as_mut_ptr().add(index);
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
            ret
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
        debug_assert!(node.len() > C);
        Self { node }
    }

    pub fn children(&self) -> &'a [Option<Node<T, B, C>>; B] {
        debug_assert!(self.node.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { &self.node.ptr.children }
    }
}

pub struct InternalMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, T, B, C> {
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

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len > C);
        self.node.length = NonZeroUsize::new(new_len).unwrap();
    }

    pub fn children(&self) -> &[Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { &self.node.ptr.children }
    }

    pub fn children_mut(&mut self) -> &mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { &mut self.node.ptr.children }
    }

    pub fn into_children_mut(self) -> &'a mut [Option<Node<T, B, C>>; B] {
        debug_assert!(self.len() > C);
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { &mut self.node.ptr.children }
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

    fn find_index(&mut self, mut index: usize) -> (usize, usize) {
        for (i, maybe_child) in self.children().iter().enumerate() {
            if let Some(child) = maybe_child {
                if index < child.len() {
                    return (i, index);
                }
                index -= child.len();
            }
        }
        unreachable!();
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Node<T, B, C>> {
        let (self_index, child_index) = self.find_insert_index(index);

        if let Some(new_child) = self.children_mut()[self_index]
            .as_mut()
            .and_then(|n| n.insert(child_index, value))
        {
            if self.is_full() {
                unsafe {
                    return Some(self.split_and_insert_node(self_index + 1, new_child));
                }
            }
            self.insert_fitting(self_index + 1, new_child);
        }

        unsafe { self.set_len(self.len() + 1) };
        None
    }

    fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        slice_insert_forget_last(self.children_mut(), index, Some(node));
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

        let new_self_len = sum_lens(&self.children_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_mut()[split_index..].swap_with_slice(&mut new_box[..tail_len]);

        slice_insert_forget_last(&mut self.children_mut()[..=split_index], index, Some(node));

        debug_assert_eq!(new_self_len + node_len, sum_lens(self.children()));
        debug_assert_eq!(new_nodes_len + 1, sum_lens(new_box.as_ref()));
        unsafe { self.set_len(new_self_len + node_len) };
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

        let new_self_len = sum_lens(&self.children_mut()[..split_index]);
        let new_nodes_len = self.len() - node_len - new_self_len;

        self.children_mut()[split_index..index].swap_with_slice(&mut new_box[..tail_start_len]);
        self.children_mut()[index..].swap_with_slice(&mut new_box[tail_start_len + 1..=tail_len]);
        new_box[tail_start_len] = Some(node);

        debug_assert_eq!(new_self_len, sum_lens(self.children()));
        debug_assert_eq!(new_nodes_len + node_len + 1, sum_lens(new_box.as_ref()));
        unsafe { self.set_len(new_self_len) };
        Node::from_children(new_nodes_len + node_len + 1, new_box)
    }

    pub unsafe fn remove(&mut self, index: usize) -> RemoveResult<T> {
        let (mut self_index, child_index) = self.find_index(index);
        debug_assert_eq!(self.len(), sum_lens(self.children()));

        match self.children_mut()[self_index]
            .as_mut()
            .unwrap()
            .variant_mut()
        {
            VariantMut::Internal { mut handle } => match unsafe { handle.remove(child_index) } {
                RemoveResult::Ok(val) => {
                    unsafe {
                        self.set_len(self.len() - 1);
                    }
                    debug_assert_eq!(self.len(), sum_lens(self.children()));
                    RemoveResult::Ok(val)
                }
                RemoveResult::WithVacancy(val, vacant_index) => {
                    debug_assert!(handle.children()[vacant_index].is_none());
                    slice_shift_left(&mut handle.children_mut()[vacant_index..], None);

                    let ret = if handle.children()[B / 2].is_none() {
                        if self_index == 0 {
                            self_index += 1;
                        }

                        let [ref mut fst, ref mut snd]: &mut [Option<Node<T, B, C>>; 2] =
                            (&mut self.children_mut()[self_index - 1..=self_index])
                                .try_into()
                                .unwrap();

                        match unsafe { combine_internals(fst, snd) } {
                            CombineResult::Ok => RemoveResult::Ok(val),
                            CombineResult::Merged => {
                                debug_assert!(self.children()[self_index].is_none());
                                debug_assert!(self.children()[self_index - 1].is_some());
                                RemoveResult::WithVacancy(val, self_index)
                            }
                        }
                    } else {
                        RemoveResult::Ok(val)
                    };

                    unsafe {
                        self.set_len(self.len() - 1);
                    }

                    debug_assert_eq!(self.len(), sum_lens(self.children()));
                    ret
                }
            },
            VariantMut::Leaf { mut handle } => {
                if handle.len() - 1 > C / 2 {
                    let val = unsafe { handle.remove_no_underflow(child_index) };
                    unsafe {
                        handle.set_len(handle.len() - 1);
                        self.set_len(self.len() - 1);
                    }
                    debug_assert_eq!(self.len(), sum_lens(self.children()));
                    return RemoveResult::Ok(val);
                }

                let ret;
                if self_index > 0 {
                    let [prev, cur]: &mut [Option<Node<T, B, C>>; 2] = (&mut self.children_mut()
                        [self_index - 1..=self_index])
                        .try_into()
                        .unwrap();

                    if prev.as_ref().unwrap().len() == C / 2 + 1 {
                        let dst = prev.as_mut().unwrap();
                        let mut src = cur.take().unwrap();
                        let dst_ptr = unsafe { (*dst.ptr.values).as_mut_ptr().add(C / 2 + 1) };
                        let src_ptr = unsafe { src.ptr.values.as_ptr() };

                        let val = unsafe { ptr::read(src_ptr.add(child_index)).assume_init() };

                        unsafe {
                            ptr::copy_nonoverlapping(src_ptr, dst_ptr, child_index);
                            ptr::copy_nonoverlapping(
                                src_ptr.add(child_index + 1),
                                dst_ptr.add(child_index),
                                C / 2 - child_index,
                            );
                            ManuallyDrop::drop(&mut src.ptr.values); 
                            mem::forget(src);
                        }

                        dst.length = NonZeroUsize::new(C).unwrap();
                        ret = RemoveResult::WithVacancy(val, self_index);
                    } else {
                        unsafe {
                            let mut prev = LeafMut::new(prev.as_mut().unwrap());
                            let cur = cur.as_mut().unwrap();

                            let x = prev.pop_back();
                            let val = cur.ptr.values[child_index].as_ptr().read();
                            let cur_ptr = (*cur.ptr.values).as_mut_ptr();
                            ptr::copy(cur_ptr, cur_ptr.add(1), child_index);
                            (*cur.ptr.values)[0].write(x);
                            ret = RemoveResult::Ok(val);
                        }
                    }
                } else {
                    let [cur, next]: &mut [Option<Node<T, B, C>>; 2] =
                        (&mut self.children_mut()[0..=1]).try_into().unwrap();

                    if next.as_ref().unwrap().len() == C / 2 + 1 {
                        let dst = cur.as_mut().unwrap();
                        let mut src = next.take().unwrap();
                        let dst_ptr = unsafe { (*dst.ptr.values).as_mut_ptr() };
                        let src_ptr = unsafe { src.ptr.values.as_ptr() };

                        let val = unsafe { ptr::read(dst_ptr.add(child_index)).assume_init() };

                        unsafe {
                            ptr::copy(
                                dst_ptr.add(child_index + 1),
                                dst_ptr.add(child_index),
                                dst.len() - child_index - 1,
                            );
                            ptr::copy_nonoverlapping(
                                src_ptr,
                                dst_ptr.add(dst.len() - 1),
                                src.len(),
                            );
                            ManuallyDrop::drop(&mut src.ptr.values);
                        }
                        mem::forget(src);

                        dst.length = NonZeroUsize::new(C).unwrap();
                        ret = RemoveResult::WithVacancy(val, 1);
                    } else {
                        unsafe {
                            let mut next = LeafMut::new(next.as_mut().unwrap());
                            let cur = cur.as_mut().unwrap();

                            let x = next.pop_front();
                            let val = cur.ptr.values[child_index].as_ptr().read();
                            let cur_ptr = (*cur.ptr.values).as_mut_ptr();
                            ptr::copy(
                                cur_ptr.add(child_index),
                                cur_ptr.add(child_index + 1),
                                C / 2 - child_index,
                            );
                            (*cur.ptr.values)[C / 2].write(x);
                            ret = RemoveResult::Ok(val);
                        }
                    }
                };

                unsafe {
                    // FIXME: when B < 5
                    self.set_len(self.len() - 1);
                }
                debug_assert_eq!(self.len(), sum_lens(self.children()));
                ret
            }
        }
    }
}

pub enum RemoveResult<T> {
    Ok(T),
    WithVacancy(T, usize),
}

enum CombineResult {
    Ok,
    Merged,
}

unsafe fn combine_internals<T, const B: usize, const C: usize>(
    opt_fst: &mut Option<Node<T, B, C>>,
    opt_snd: &mut Option<Node<T, B, C>>,
) -> CombineResult {
    let mut fst = unsafe { InternalMut::new(opt_fst.as_mut().unwrap()) };
    let mut snd = unsafe { InternalMut::new(opt_snd.as_mut().unwrap()) };
    let fst_underfull = fst.children()[B / 2].is_none();
    let snd_underfull = snd.children()[B / 2].is_none();
    let fst_almost_underfull = fst.children()[B / 2 + 1].is_none();
    let snd_almost_underfull = snd.children()[B / 2 + 1].is_none();

    if fst_underfull && snd_almost_underfull {
        fst.children_mut()[B / 2..].swap_with_slice(&mut snd.children_mut()[..=B / 2]);
        unsafe {
            fst.set_len(fst.len() + snd.len());
        }
        *opt_snd = None;
        debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        return CombineResult::Merged;
    }

    if fst_almost_underfull && snd_underfull {
        fst.children_mut()[B / 2 + 1..].swap_with_slice(&mut snd.children_mut()[..B / 2]);
        unsafe {
            fst.set_len(fst.len() + snd.len());
        }
        *opt_snd = None;
        debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        return CombineResult::Merged;
    }

    if fst_underfull {
        let x = slice_shift_left(snd.children_mut(), None).unwrap();
        unsafe {
            snd.set_len(snd.len() - x.len());
            fst.set_len(fst.len() + x.len());
        }
        fst.children_mut()[B / 2] = Some(x);
        debug_assert_eq!(fst.len(), sum_lens(fst.children()));
        debug_assert_eq!(snd.len(), sum_lens(snd.children()));
        return CombineResult::Ok;
    }

    if snd_underfull {
        let mut i = B - 1;
        loop {
            if fst.children_mut()[i].is_some() {
                let x = fst.children_mut()[i].take().unwrap();

                unsafe {
                    fst.set_len(fst.len() - x.len());
                    snd.set_len(snd.len() + x.len());
                }

                slice_shift_right(snd.children_mut(), Some(x));
                debug_assert_eq!(fst.len(), sum_lens(fst.children()));
                debug_assert_eq!(snd.len(), sum_lens(snd.children()));
                return CombineResult::Ok;
            }
            i -= 1;
        }
    }

    CombineResult::Ok
}

fn sum_lens<T, const B: usize, const C: usize>(children: &[Option<Node<T, B, C>>]) -> usize {
    children
        .iter()
        .map(|n| n.as_ref().map_or(0, Node::len))
        .sum()
}
