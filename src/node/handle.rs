use core::{
    marker::PhantomData,
    ptr::NonNull,
};

use crate::{
    node::{InternalNode, LeafBase, LeafBox, NodeBase, NodePtr, BRANCH_FACTOR},
    ownership::{self, Reference},
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
        unsafe { &*self.array_ptr().add(index) }
    }
}

pub struct LeafPtr<R, T>
where
    R: Reference<T>,
{
    pub node: NonNull<LeafBase<T>>,
    marker: PhantomData<R>,
}

pub type LeafRef<'a, T> = LeafPtr<ownership::Immut<'a>, T>;
pub type LeafMut<'a, T> = LeafPtr<ownership::Mut<'a>, T>;

impl<T> LeafBox<T> {
    pub fn as_ref(&self) -> LeafRef<T> {
        unsafe { LeafPtr::new(self.ptr) }
    }

    pub fn as_mut(&mut self) -> LeafMut<T> {
        unsafe { LeafPtr::new(self.ptr) }
    }
}

impl<'a, T: 'a> Clone for LeafRef<'a, T> {
    fn clone(&self) -> Self {
        Self {
            node: self.node,
            marker: self.marker,
        }
    }
}

impl<R, T> LeafPtr<R, T>
where
    R: Reference<T>,
{
    pub unsafe fn new(ptr: NonNull<LeafBase<T>>) -> Self {
        Self {
            node: ptr,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        unsafe { usize::from(self.node.as_ref().len) }
    }

    pub fn array_ptr(&self) -> *mut T {
        let (_, array_offset) = NodeBase::<T>::leaf_layout();
        unsafe {
            self.node
                .as_ptr()
                .cast::<u8>()
                // SAFETY: array_offset is inbounds of
                // a Leaf according to leaf_layout()
                .add(array_offset)
                .cast::<T>()
        }
    }
}

impl<'a, T: 'a> LeafMut<'a, T> {
    pub fn values_mut(&mut self) -> ArrayVecMut<T> {
        let array = self.array_ptr();
        unsafe {
            ArrayVecMut::new(
                array,
                &mut (*self.node.as_ptr()).len,
                NodeBase::<T>::LEAF_CAP as u16,
            )
        }
    }

    pub unsafe fn into_value_unchecked_mut(self, index: usize) -> &'a mut T {
        let len = self.len();
        debug_assert!(len <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < len);
        unsafe { &mut *self.array_ptr().add(index) }
    }

    pub unsafe fn value_unchecked_mut(&mut self, index: usize) -> &mut T {
        let len = self.len();
        debug_assert!(len <= NodeBase::<T>::LEAF_CAP);
        debug_assert!(index < len);
        unsafe { &mut *self.array_ptr().add(index) }
    }

    pub fn is_full(&self) -> bool {
        self.len() == NodeBase::<T>::LEAF_CAP
    }
}

impl<T> InternalNode<T> {
    pub const UNDERFULL_LEN: usize = (BRANCH_FACTOR - 1) / 2;

    pub unsafe fn child_pair_at(&mut self, index: usize) -> [NodePtr<T>; 2] {
        let ptr = self.children.as_mut_ptr();
        [unsafe { ptr.add(index).read().assume_init() }, unsafe {
            ptr.add(index + 1).read().assume_init()
        }]
    }

    pub fn is_full(&self) -> bool {
        self.len_children() == BRANCH_FACTOR
    }

    pub fn is_underfull(&self) -> bool {
        self.len_children() <= Self::UNDERFULL_LEN
    }

    pub fn is_singleton(&self) -> bool {
        self.len_children() == 1
    }

    pub fn len(&self) -> usize {
        self.lengths.total_len()
    }

    pub fn len_children(&self) -> usize {
        self.children_len.into()
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
}
