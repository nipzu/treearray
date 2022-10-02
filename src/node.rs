use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

use self::handle::InternalMut;

// use crate::panics::panic_length_overflow;

#[cfg(miri)]
const BRANCH_FACTOR: usize = 4;
#[cfg(not(miri))]
const BRANCH_FACTOR: usize = 32;

pub mod handle;

pub struct RawNodeWithLen<T, const C: usize>(pub usize, pub NodePtr<T, C>);

pub type NodePtr<T, const C: usize> = NonNull<NodeBase<T, C>>;

pub struct NodeBase<T, const C: usize> {
    pub parent: Option<NodePtr<T, C>>,
    pub parent_index: MaybeUninit<u16>,
    pub children_len: u16,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T, const C: usize> {
    base: NodeBase<T, C>,
    pub lengths: [usize; BRANCH_FACTOR],
    pub children: [MaybeUninit<NodePtr<T, C>>; BRANCH_FACTOR],
}

#[repr(C)]
pub struct LeafNode<T, const C: usize> {
    base: NodeBase<T, C>,
    values: [MaybeUninit<T>; C],
}

impl<T, const C: usize> NodeBase<T, C> {
    pub const fn new() -> Self {
        Self {
            parent: None,
            parent_index: MaybeUninit::uninit(),
            children_len: 0,
            _marker: PhantomData,
        }
    }
}

impl<T, const C: usize> LeafNode<T, C> {
    const UNINIT_T: MaybeUninit<T> = MaybeUninit::uninit();

    pub fn new() -> NodePtr<T, C> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            values: [Self::UNINIT_T; C],
        })))
        .cast()
    }

    pub fn from_value(value: T) -> NodePtr<T, C> {
        let mut leaf = Self::new();
        unsafe {
            leaf.cast::<LeafNode<T, C>>().as_mut().values.as_mut()[0].write(value);
            leaf.as_mut().children_len = 1;
        };
        leaf
    }
}

impl<T, const C: usize> InternalNode<T, C> {
    const UNINIT_NODE: MaybeUninit<NodePtr<T, C>> = MaybeUninit::uninit();

    pub fn new() -> NodePtr<T, C> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            lengths: [0; BRANCH_FACTOR],
            children: [Self::UNINIT_NODE; BRANCH_FACTOR],
        })))
        .cast()
    }

    pub fn from_child_array<const N: usize>(children: [RawNodeWithLen<T, C>; N]) -> NodePtr<T, C> {
        let boxed_children = Self::new();
        let mut children_mut = unsafe { InternalMut::new(boxed_children) };
        for child in children {
            unsafe { children_mut.push_back_child(child) }
        }

        boxed_children
    }
}
