use core::{hint::unreachable_unchecked, mem::MaybeUninit};

use alloc::boxed::Box;

use crate::{
    node::{Children, Node},
    utils::ArrayVecMut,
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

    pub fn value(&self, index: usize) -> Option<&'a T> {
        (index < self.len()).then(|| unsafe { self.value_unchecked(index) })
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

    pub fn free(mut self) {
        debug_assert_eq!(self.len(), 0);
        unsafe { Box::from_raw(self.values_maybe_uninit_mut()) };
    }

    pub fn values_mut(&mut self) -> ArrayVecMut<T, C> {
        unsafe { ArrayVecMut::new(self.node.ptr.values.as_mut(), &mut self.node.length) }
    }

    pub fn rotate_from_previous(&mut self, mut prev: LeafMut<T, B, C>) {
        self.values_mut().push_front(prev.values_mut().pop_back());
    }

    pub fn rotate_from_next(&mut self, mut next: LeafMut<T, B, C>) {
        self.values_mut().push_back(next.values_mut().pop_front());
    }

    pub fn append_from(&mut self, mut other: Self) {
        self.values_mut().append(other.values_mut());
        other.free();
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

        if self.is_full() {
            InsertResult::Split(if index <= C / 2 {
                SplitResult::Left(self.split_and_insert_left(index, value))
            } else {
                SplitResult::Right(self.split_and_insert_right(index, value))
            })
        } else {
            self.values_mut().insert(index, value);
            InsertResult::Fit
        }
    }

    fn split_and_insert_left(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let split_index = C / 2;
        let mut new_node = Node::empty_leaf();
        let mut new_leaf = unsafe { LeafMut::new(&mut new_node) };
        let mut values = self.values_mut();
        values.split(split_index, new_leaf.values_mut());
        values.insert(index, value);
        new_node
    }

    fn split_and_insert_right(&mut self, index: usize, value: T) -> Node<T, B, C> {
        let split_index = (C - 1) / 2 + 1;
        let mut new_node = Node::empty_leaf();
        let mut new_leaf = unsafe { LeafMut::new(&mut new_node) };
        self.values_mut().split(split_index, new_leaf.values_mut());
        new_leaf.values_mut().insert(index - self.len(), value);
        new_node
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

    pub fn children(&self) -> &'a Children<T, B, C> {
        // SAFETY: `self.node` is guaranteed to be a child node by the safety invariants of
        // `Self::new`, so the `children` field of the `self.node.ptr` union can be read.
        unsafe { self.node.ptr.children.as_ref() }
    }

    pub unsafe fn child_containing_index(&self, index: &mut usize) -> &'a Node<T, B, C> {
        for child in self.children().children() {
            let len = child.len();
            match index.checked_sub(len) {
                Some(r) => *index = r,
                None => return child,
            }
        }

        unsafe { unreachable_unchecked() };
    }

    pub unsafe fn index_of_child_ptr(&self, child_ptr: *const Node<T, B, C>) -> usize {
        let slice_ptr = unsafe { self.node.ptr.children.as_ptr() };
        #[allow(clippy::cast_sign_loss)]
        unsafe {
            child_ptr.offset_from(slice_ptr.cast()) as usize
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

    pub fn count_children(&self) -> usize {
        self.children().len
    }

    pub fn is_singleton(&self) -> bool {
        self.count_children() == 1
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

    pub fn children_mut(&mut self) -> ArrayVecMut<Node<T, B, C>, B> {
        self.raw_children_mut().as_array_vec()
    }

    pub fn raw_children_mut(&mut self) -> &mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn into_children_mut(self) -> &'a mut Children<T, B, C> {
        unsafe { self.node.ptr.children.as_mut() }
    }

    pub fn into_node(self) -> &'a mut Node<T, B, C> {
        self.node
    }

    pub fn free(self) {
        debug_assert_eq!(self.children().children().len(), 0);
        unsafe { Box::from_raw(self.into_children_mut()) };
    }

    pub fn child_mut(&mut self, index: usize) -> &mut Node<T, B, C> {
        &mut self.raw_children_mut().children_mut()[index]
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

    unsafe fn insert_fitting(&mut self, index: usize, node: Node<T, B, C>) {
        debug_assert!(!self.is_full());
        self.children_mut().insert(index, node);

        // TODO: this logic is poor
        self.set_len(self.len() + 1);
    }

    unsafe fn split_and_insert_left(&mut self, index: usize, node: Node<T, B, C>) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN;

        let new_sibling = self.raw_children_mut().split(split_index);
        self.children_mut().insert(index, node);

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;

        debug_assert_eq!(new_node_len, new_sibling.sum_lens());

        self.set_len(new_self_len);
        Node::from_children(new_node_len, new_sibling)
    }

    unsafe fn split_and_insert_right(
        &mut self,
        index: usize,
        node: Node<T, B, C>,
    ) -> Node<T, B, C> {
        let split_index = Self::UNDERFULL_LEN + 1;
        let mut new_sibling;

        new_sibling = self.raw_children_mut().split(split_index);
        new_sibling.as_array_vec().insert(index - split_index, node);

        let new_self_len = self.children().sum_lens();
        let new_node_len = self.len() - new_self_len + 1;

        debug_assert_eq!(new_node_len, new_sibling.sum_lens());

        self.set_len(new_self_len);
        Node::from_children(new_node_len, new_sibling)
    }

    pub fn reborrow(&mut self) -> InternalMut<T, B, C> {
        InternalMut { node: self.node }
    }

    pub unsafe fn append_from(&mut self, mut other: InternalMut<T, B, C>) {
        self.children_mut().append(other.children_mut());

        self.set_len(self.len() + other.len());
        other.free();
    }

    pub unsafe fn rotate_from_next(&mut self, mut next: InternalMut<T, B, C>) {
        let x = next.children_mut().pop_front();

        next.set_len(next.len() - x.len());
        self.set_len(self.len() + x.len());

        self.children_mut().push_back(x);
    }

    pub unsafe fn rotate_from_previous(&mut self, mut prev: InternalMut<T, B, C>) {
        let x = prev.children_mut().pop_back();

        prev.set_len(prev.len() - x.len());
        self.set_len(self.len() + x.len());

        self.children_mut().push_front(x);
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
