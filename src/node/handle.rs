use core::{hint::unreachable_unchecked, mem::MaybeUninit, ptr};

use alloc::boxed::Box;

use crate::{
    node::{Children, Node},
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

    pub fn len(&self) -> usize {
        self.node.len()
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= C);
        self.node.length = new_len;
    }

    pub fn free(mut self) {
        debug_assert_eq!(self.len(), 0);
        unsafe { Box::from_raw(self.values_maybe_uninit_mut()) };
    }

    pub unsafe fn pop_back(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe { self.remove_unchecked(self.len() - 1) }
    }

    pub unsafe fn pop_front(&mut self) -> T {
        debug_assert!(self.len() > 0);
        unsafe { self.remove_unchecked(0) }
    }

    pub unsafe fn push_front(&mut self, value: T) {
        debug_assert!(!self.is_full());
        self.insert_fitting(0, value);
    }

    pub unsafe fn push_back(&mut self, value: T) {
        debug_assert!(!self.is_full());
        unsafe {
            let len = self.len();
            self.values_maybe_uninit_mut()[len].write(value);
            self.set_len(len + 1);
        }
    }

    pub unsafe fn append_from(&mut self, mut other: Self) {
        // TODO: debug_asserts
        let self_len = self.len();
        let other_len = other.len();
        debug_assert!(self_len + other_len <= C);
        let self_ptr = unsafe { self.values_maybe_uninit_mut().as_mut_ptr().add(self_len) };
        let other_ptr = other.values_maybe_uninit_mut().as_ptr();

        unsafe {
            ptr::copy_nonoverlapping(other_ptr, self_ptr, other_len);
            self.set_len(self_len + other_len);
            other.set_len(0);
            other.free();
        }
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        let len = self.len();
        debug_assert!(len <= C);
        unsafe { slice_assume_init_mut(self.values_maybe_uninit_mut().get_unchecked_mut(..len)) }
    }

    pub fn values_maybe_uninit_mut(&mut self) -> &mut [MaybeUninit<T>; C] {
        unsafe { self.node.ptr.values.as_mut() }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= C);
        debug_assert!(index < len);
        unsafe {
            self.node
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

    pub fn is_underfull(&self) -> bool {
        self.len() < (C - 1) / 2 + 1
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
        assert!(!self.is_full());
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
        unsafe { self.children()[0].assume_init_ref().len() == self.node.len() }
    }

    pub fn children(&self) -> &'a [MaybeUninit<Node<T, B, C>>; B] {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { &self.node.ptr.children.as_ref().children }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> &'a Node<T, B, C> {
        fn child_len<T, const B: usize, const C: usize>(
            child: &MaybeUninit<Node<T, B, C>>,
        ) -> usize {
            unsafe { child.assume_init_ref().len() }
        }

        for child in self.children() {
            let len = child_len(child);
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return unsafe { child.assume_init_ref() },
            }
        }

        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, elem_ptr: *const Node<T, B, C>) -> usize {
        let slice_ptr = unsafe { self.node.ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            elem_ptr.offset_from(slice_ptr.cast()) as usize
        }
    }
}

pub struct InternalMut<'a, T, const B: usize, const C: usize> {
    node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> InternalMut<'a, T, B, C> {
    pub const UNDERFULL_LEN: usize = (B - 1) / 2;

    /// # Safety:
    ///
    /// `node` must be a child node i.e. `node.len() > C`.
    pub unsafe fn new(node: &'a mut Node<T, B, C>) -> Self {
        Self { node }
    }

    pub unsafe fn from_ptr(ptr: *mut Node<T, B, C>) -> Self {
        unsafe { Self::new(&mut *ptr) }
    }

    pub fn count_children(&self) -> usize {
        self.children().len
    }

    fn is_full(&self) -> bool {
        self.count_children() == B
    }

    pub fn is_underfull(&self) -> bool {
        self.count_children() <= Self::UNDERFULL_LEN
    }

    pub fn is_almost_underfull(&self) -> bool {
        self.count_children() <= Self::UNDERFULL_LEN + 1
    }

    pub fn len(&self) -> usize {
        self.node.len()
    }

    pub fn set_len(&mut self, new_len: usize) {
        self.node.length = new_len;
    }

    pub unsafe fn into_child_containing_index(self, index: &mut usize) -> &'a mut Node<T, B, C> {
        for child in self.into_children_mut().children_mut() {
            // let child = unsafe { child.assume_init_mut() };
            let len = child.len();
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        debug_assert!(false);
        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, elem_ptr: *const Node<T, B, C>) -> usize {
        let slice_ptr = unsafe { self.node.ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            elem_ptr.offset_from(slice_ptr.cast()) as usize
        }
    }

    pub fn children(&self) -> &Children<T, B, C> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub fn children_mut(&mut self) -> &mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn into_children_mut(self) -> &'a mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn free(mut self) {
        debug_assert_eq!(self.children().children().len(), 0);
        unsafe { Box::from_raw(self.children_mut()) };
    }

    pub fn child_mut(&mut self, index: usize) -> &mut Node<T, B, C> {
        &mut self.children_mut().children_mut()[index]
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
        unsafe {
            self.children_mut().insert(index, node);
        }
        // TODO: this logic is poor
        self.set_len(self.len() + 1);
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        // let node_len = node.len();
        let split_index = Self::UNDERFULL_LEN;
        let new_sibling;

        unsafe {
            new_sibling = self.children_mut().split(split_index);
            self.children_mut().insert(index, node);
        }

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;
        // let new_node_len = new_sibling.sum_lens();

        // debug_assert_eq!(new_self_len + node_len, self.children().sum_lens());
        // debug_assert_eq!(new_node_len + 1, new_sibling.sum_lens());

        self.set_len(new_self_len);
        Node::from_children(new_node_len, new_sibling)
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        // let node_len = node.len();
        let split_index = Self::UNDERFULL_LEN + 1;
        let mut new_sibling;

        unsafe {
            new_sibling = self.children_mut().split(split_index);
            new_sibling.insert(index - split_index, node);
        }

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;
        // let new_node_len = new_sibling.sum_lens();

        // debug_assert_eq!(new_self_len + node_len, self.children().sum_lens());
        // debug_assert_eq!(new_node_len + 1, new_sibling.sum_lens());

        self.set_len(new_self_len);
        Node::from_children(new_node_len, new_sibling)
    }

    pub fn reborrow(&mut self) -> InternalMut<T, B, C> {
        InternalMut { node: self.node }
    }

    pub unsafe fn append_from(&mut self, mut other: InternalMut<T, B, C>) {
        unsafe {
            self.children_mut().merge_with_next(other.children_mut());
        }

        self.set_len(self.len() + other.len());
        other.free();
    }

    pub unsafe fn rotate_from_next(&mut self, mut next: InternalMut<T, B, C>) {
        let x = unsafe { next.children_mut().pop_front() };

        next.set_len(next.len() - x.len());
        self.set_len(self.len() + x.len());

        unsafe {
            self.children_mut().push_back(x);
        }
    }

    pub unsafe fn rotate_from_previous(&mut self, mut prev: InternalMut<T, B, C>) {
        let x = unsafe { prev.children_mut().pop_back() };

        prev.set_len(prev.len() - x.len());
        self.set_len(self.len() + x.len());

        unsafe {
            self.children_mut().push_front(x);
        }
    }
}

pub enum InsertResult<T, const B: usize, const C: usize> {
    Fit,
    Split(SplitResult<T, B, C>),
}

pub enum SplitResult<T, const B: usize, const C: usize> {
    Left(Node<T, B, C>),
    Right(Node<T, B, C>),
}
