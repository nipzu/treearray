use core::{
    marker::PhantomData,
    ptr::{addr_of_mut, NonNull},
};

use crate::{
    node::{InternalNode, LeafBase, NodeBase, NodePtr, BRANCH_FACTOR},
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

    pub fn is_full(&self) -> bool {
        self.len() == NodeBase::<T>::LEAF_CAP
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

impl<T> InternalNode<T> {
    pub const UNDERFULL_LEN: usize = (BRANCH_FACTOR - 1) / 2;
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

pub unsafe fn free_internal<T>(mut ptr: NodePtr<T>) {
    unsafe {
        debug_assert_eq!(ptr.internal_mut().children_len, 0);
        // debug_assert_eq!(ptr.internal_mut().lengths.total_len(), 0);
        drop(ptr.into_internal());
    }
}
