use core::{
    marker::PhantomData,
    mem,
    ptr::{self, addr_of_mut, NonNull},
};

use crate::{
    node::{
        fenwick::FenwickTree, InternalNode, LeafBase, NodeBase, NodePtr, RawNodeWithLen,
        BRANCH_FACTOR,
    },
    ownership,
    utils::ArrayVecMut,
};

impl<'a, T: 'a> LeafRef<'a, T> {
    pub unsafe fn value_unchecked(&self, index: usize) -> &'a T {
        debug_assert!(self.len() <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < self.len());

        // We own a shared reference to this leaf, so there
        // should not be any mutable references which
        // could cause aliasing problems with taking
        // a reference to the whole array.
        unsafe {
            let (_, array_offset) = NodeBase::<T>::leaf_layout();
            &*self
                .node
                .as_ptr()
                .cast::<u8>()
                .add(array_offset)
                .cast::<T>()
                .add(index)
        }
    }
}

pub struct LeafPtr<O, T>
where
    O: ownership::Ownership<T>,
{
    pub node: NonNull<LeafBase<T>>,
    _marker: PhantomData<O>,
}

pub type LeafRef<'a, T> = LeafPtr<ownership::Immut<'a>, T>;
pub type LeafMut<'a, T> = LeafPtr<ownership::Mut<'a>, T>;
pub type Leaf<T> = LeafPtr<ownership::Owned, T>;

impl<'a, T: 'a> Clone for LeafRef<'a, T> {
    fn clone(&self) -> Self {
        Self {
            node: self.node,
            _marker: self._marker,
        }
    }
}

impl<O, T> LeafPtr<O, T>
where
    O: ownership::Ownership<T>,
{
    pub unsafe fn new(ptr: NonNull<LeafBase<T>>) -> Self {
        Self {
            node: ptr,
            _marker: PhantomData,
        }
    }
}

impl<O, T> LeafPtr<O, T>
where
    O: ownership::Ownership<T>,
{
    pub fn len(&self) -> usize {
        unsafe { usize::from(self.node.as_ref().len) }
    }
}

impl<'a, T: 'a> LeafMut<'a, T> {
    pub fn values_mut(&mut self) -> ArrayVecMut<T> {
        unsafe {
            let (_, offset) = NodeBase::<T>::leaf_layout();
            let array = self.node.as_ptr().cast::<u8>().add(offset).cast();
            ArrayVecMut::new(
                array,
                addr_of_mut!((*self.node.as_ptr()).len),
                NodeBase::<T>::LEAF_CAP as u16,
            )
        }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < len);
        unsafe {
            let (_, offset) = NodeBase::<T>::leaf_layout();
            &mut *self
                .node
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .add(index)
        }
    }

    fn is_full(&self) -> bool {
        self.len() == NodeBase::<T>::LEAF_CAP
    }

    pub fn insert_value(&mut self, index: usize, value: T) -> Option<RawNodeWithLen<T>> {
        assert!(index <= self.len());

        if let Some(mut new_sibling_node) = self.split_if_full() {
            if index <= self.len() {
                self.values_mut().insert(index, value);
            } else {
                unsafe { LeafMut::new(new_sibling_node.1.leaf).values_mut().insert(index - self.len(), value) };
                new_sibling_node.0 += 1;
            }

            Some(new_sibling_node)
        } else {
            self.values_mut().insert(index, value);
            None
        }
    }

    fn split_if_full(&mut self) -> Option<RawNodeWithLen<T>> {
        self.is_full().then(|| {
            let split_index = NodeBase::<T>::LEAF_CAP / 2;
            let mut new_node = NodeBase::new_leaf();
            let mut new_leaf = unsafe { LeafMut::new(new_node.leaf) };
            self.values_mut().split(split_index, new_leaf.values_mut());

            unsafe {
                let old_next = (*self.node.as_ptr()).next;
                (*self.node.as_ptr()).next = Some(new_node.leaf);
                new_node.leaf.as_mut().next = old_next;
                new_node.leaf.as_mut().prev = Some(self.node);
                if let Some(next_of_next) = old_next {
                    (*next_of_next.as_ptr()).prev = Some(new_node.leaf);
                }
            };

            RawNodeWithLen(new_leaf.len(), new_node)
        })
    }
}

impl<T> InternalNode<T> {
    /*pub unsafe fn child_mut(&mut self, index: usize) -> LeafMut<T> {
        let ptr = unsafe { (*self.internal_ptr()).children.as_mut_ptr() };
        unsafe { LeafMut::new(ptr.add(index).read().assume_init()) }
    }*/

    pub unsafe fn child_pair_at(&mut self, index: usize) -> [NodePtr<T>; 2] {
        let ptr = self.children.as_mut_ptr();
        [unsafe { ptr.add(index).read().assume_init() }, unsafe {
            ptr.add(index + 1).read().assume_init()
        }]
    }

    pub fn handle_underfull_leaf_child_head(&mut self) {
        let [cur, next] = unsafe { self.child_pair_at(0) };
        let [mut cur, mut next] = unsafe { [LeafMut::new(cur.leaf), LeafMut::new(next.leaf)] };

        unsafe {
            if next.is_almost_underfull() {
                cur.values_mut().append(next.values_mut());
                self.merge_length_from_next(0);
                Leaf::new(self.children().remove(1).leaf).free();
            } else {
                cur.push_back_child(next.pop_front_child());
                self.steal_length_from_next(0, 1);
            }
        }
    }

    pub fn handle_underfull_leaf_child_tail(&mut self, index: usize) {
        let [prev, cur] = unsafe { self.child_pair_at(index - 1) };
        let [mut prev, mut cur] = unsafe { [LeafMut::new(prev.leaf), LeafMut::new(cur.leaf)] };

        unsafe {
            if prev.is_almost_underfull() {
                prev.values_mut().append(cur.values_mut());
                self.merge_length_from_next(index - 1);
                Leaf::new(self.children().remove(index).leaf).free();
            } else {
                cur.push_front_child(prev.pop_back_child());
                self.steal_length_from_previous(index, 1);
            }
        }
    }

    pub fn maybe_handle_underfull_child(&mut self, index: usize) -> bool {
        let is_child_underfull = unsafe {
            self.children[index]
                .assume_init_mut()
                .internal_mut()
                .is_underfull()
        };

        if is_child_underfull {
            if index > 0 {
                self.handle_underfull_internal_child_tail(index);
            } else {
                self.handle_underfull_internal_child_head();
            }
        }

        is_child_underfull
    }

    fn handle_underfull_internal_child_head(&mut self) {
        let [mut cur, mut next] = unsafe { self.child_pair_at(0) };
        let [cur, next] = unsafe { [cur.internal_mut(), next.internal_mut()] };

        if next.is_almost_underfull() {
            unsafe {
                cur.append_children(next);
                self.merge_length_from_next(0);
                free_internal(self.children().remove(1));
            }
        } else {
            unsafe {
                let x = next.pop_front_child();
                let x_len = x.0;
                cur.push_back_child(x);
                self.steal_length_from_next(0, x_len);
            }
        }
    }

    fn handle_underfull_internal_child_tail(&mut self, index: usize) {
        let [mut prev, mut cur] = unsafe { self.child_pair_at(index - 1) };
        let [prev, cur] = unsafe { [prev.internal_mut(), cur.internal_mut()] };

        if prev.is_almost_underfull() {
            unsafe {
                prev.append_children(cur);
                self.merge_length_from_next(index - 1);
                free_internal(self.children().remove(index));
            }
        } else {
            unsafe {
                let x = prev.pop_back_child();
                let x_len = x.0;
                cur.push_front_child(x);
                self.steal_length_from_previous(index, x_len);
            }
        }
    }
}

impl<T> Leaf<T> {
    pub fn free(self) {
        unsafe {
            let ptr = self.node.as_ref();
            let next = ptr.next;
            let prev = ptr.prev;

            if let Some(p_next) = next {
                // TODO: can we take mut ref?
                debug_assert_eq!((*p_next.as_ptr()).prev, Some(self.node));
                (*p_next.as_ptr()).prev = prev;
            }

            if let Some(p_prev) = prev {
                debug_assert_eq!((*p_prev.as_ptr()).next, Some(self.node));
                (*p_prev.as_ptr()).next = next;
            }

            let (layout, _) = NodeBase::<T>::leaf_layout();
            alloc::alloc::dealloc(self.node.as_ptr().cast(), layout);
        }
    }
}

pub unsafe fn free_internal<T>(mut ptr: NodePtr<T>) {
    unsafe {
        debug_assert_eq!(ptr.internal_mut().children_len, 0);
        // debug_assert_eq!(ptr.internal_mut().lengths.total_len(), 0);
        drop(ptr.into_internal());
    }
}

impl<'a, T: 'a> LeafMut<'a, T> {
    const UNDERFULL_LEN: usize = (NodeBase::<T>::LEAF_CAP - 1) / 2;
    pub fn remove_child(&mut self, index: usize) -> T {
        self.values_mut().remove(index)
    }
    fn push_front_child(&mut self, child: T) {
        self.values_mut().insert(0, child);
    }
    fn push_back_child(&mut self, child: T) {
        let len = self.len();
        self.values_mut().insert(len, child);
    }
    fn pop_front_child(&mut self) -> T {
        self.remove_child(0)
    }
    fn pop_back_child(&mut self) -> T {
        self.remove_child(self.len() - 1)
    }
    pub fn is_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN
    }
    fn is_almost_underfull(&self) -> bool {
        self.len() <= Self::UNDERFULL_LEN + 1
    }
}

impl<T> InternalNode<T> {
    pub const UNDERFULL_LEN: usize = (BRANCH_FACTOR - 1) / 2;

    fn is_full(&self) -> bool {
        self.len_children() == BRANCH_FACTOR
    }

    pub fn is_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN
    }

    pub fn is_singleton(&self) -> bool {
        self.len_children() == 1
    }

    fn len(&self) -> usize {
        self.lengths.total_len()
    }

    fn len_children(&self) -> usize {
        self.children_len.into()
    }

    unsafe fn push_front_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe {
            self.push_front_length(child.0);
            self.children().insert(0, child.1);
        }
    }
    pub unsafe fn push_back_child(&mut self, child: RawNodeWithLen<T>) {
        unsafe { self.push_back_length(child.0) };
        let len_children = self.len_children();
        self.children().insert(len_children, child.1);
    }
    unsafe fn pop_front_child(&mut self) -> RawNodeWithLen<T> {
        let node_len = unsafe { self.pop_front_length() };
        let node = self.children().remove(0);
        RawNodeWithLen(node_len, node)
    }
    unsafe fn pop_back_child(&mut self) -> RawNodeWithLen<T> {
        let last_len = unsafe { self.pop_back_length() };
        let len_children = self.len_children();
        let last = self.children().remove(len_children - 1);
        RawNodeWithLen(last_len, last)
    }
    fn is_almost_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN + 1
    }
    unsafe fn append_children(&mut self, other: &mut InternalNode<T>) {
        unsafe { self.append_lengths(other) };
        self.children().append(other.children());
    }

    unsafe fn steal_length_from_next(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index + 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn steal_length_from_previous(&mut self, index: usize, amount: usize) {
        unsafe {
            self.add_length_wrapping(index - 1, amount.wrapping_neg());
            self.add_length_wrapping(index, amount);
        }
    }

    unsafe fn append_lengths(&mut self, other: &mut InternalNode<T>) {
        let self_len_children = self.len_children();
        let other_len_children = other.len_children();
        let other_lens = other.lengths.clone().into_array();
        self.lengths.with_flat_lens(|lens| {
            lens[self_len_children..self_len_children + other_len_children]
                .copy_from_slice(&other_lens[..other_len_children]);
        });
    }

    unsafe fn merge_length_from_next(&mut self, index: usize) {
        self.lengths.with_flat_lens(|lens| unsafe {
            let lens_ptr = lens.as_mut_ptr();
            let next_len = lens_ptr.add(index + 1).read();
            (*lens_ptr.add(index)) += next_len;
            ptr::copy(
                lens_ptr.add(index + 2),
                lens_ptr.add(index + 1),
                BRANCH_FACTOR - index - 2,
            );
            lens[BRANCH_FACTOR - 1] = 0;
        });
    }

    unsafe fn insert_length(&mut self, index: usize, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            for i in (index..len_children).rev() {
                lens[i + 1] = lens[i];
            }
            lens[index] = len;
        });
    }

    unsafe fn split_lengths(&mut self, index: usize) -> FenwickTree {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            let mut other_array = [0; BRANCH_FACTOR];
            for i in index..len_children {
                other_array[i - index] = lens[i];
                lens[i] = 0;
            }
            FenwickTree::from_array(other_array)
        })
    }

    unsafe fn pop_front_length(&mut self) -> usize {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            let first_len = lens[0];
            for i in 1..len_children {
                lens[i - 1] = lens[i];
            }
            *lens.last_mut().unwrap() = 0;
            first_len
        })
    }

    unsafe fn pop_back_length(&mut self) -> usize {
        let len_children = self.len_children();
        self.lengths
            .with_flat_lens(|lens| mem::take(&mut lens[len_children - 1]))
    }

    unsafe fn push_back_length(&mut self, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            lens[len_children] = len;
        });
    }

    unsafe fn push_front_length(&mut self, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            for i in (0..len_children).rev() {
                lens[i + 1] = lens[i];
            }
            lens[0] = len;
        });
    }

    pub unsafe fn add_length_wrapping(&mut self, index: usize, amount: usize) {
        self.lengths.add_wrapping(index, amount);
    }

    pub fn children(&mut self) -> ArrayVecMut<NodePtr<T>> {
        unsafe {
            let InternalNode {
                children_len,
                children,
                ..
            } = self;
            ArrayVecMut::new(children as *mut _ as _, children_len, BRANCH_FACTOR as u16)
        }
    }

    pub unsafe fn insert_child(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T>,
    ) -> Option<RawNodeWithLen<T>> {
        unsafe {
            if let Some(mut new_next_sibling) = self.split_if_full() {
                if index <= usize::from(self.children_len) {
                    self.insert_fitting(index, node);
                } else {
                    let n = new_next_sibling.1.internal_mut();
                    n.insert_fitting(index - usize::from(self.children_len), node);
                    new_next_sibling.0 = n.len();
                }
                Some(new_next_sibling)
            } else {
                self.insert_fitting(index, node);
                None
            }
        }
    }

    unsafe fn insert_fitting(&mut self, index: usize, node: RawNodeWithLen<T>) {
        debug_assert!(!self.is_full());
        debug_assert!(index <= usize::from(self.children_len));
        unsafe {
            self.insert_length(index, node.0);
        }
        self.children().insert(index, node.1);
    }

    fn split_if_full(&mut self) -> Option<RawNodeWithLen<T>> {
        self.is_full().then(|| {
            let split_index = Self::UNDERFULL_LEN + 1;

            let mut new_sibling_node = InternalNode::<T>::new();
            let mut new_sibling = unsafe { new_sibling_node.internal_mut() };

            unsafe {
                new_sibling.lengths = self.split_lengths(split_index);
                self.children().split(split_index, new_sibling.children());
            };

            RawNodeWithLen(new_sibling.len(), new_sibling_node)
        })
    }
}
