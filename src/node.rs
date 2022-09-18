use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

// use crate::panics::panic_length_overflow;

pub mod handle;

pub type NodePtr<T, const B: usize, const C: usize> = NonNull<NodeBase<T, B, C>>;

pub struct NodeBase<T, const B: usize, const C: usize> {
    pub parent: Option<NonNull<NodeBase<T, B, C>>>,
    pub parent_index: MaybeUninit<u16>,
    pub children_len: MaybeUninit<u16>,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T, const B: usize, const C: usize> {
    pub base: NodeBase<T, B, C>,
    pub children: [MaybeUninit<NonNull<NodeBase<T, B, C>>>; B],
    pub lengths: [MaybeUninit<usize>; B],
}

#[repr(C)]
pub struct LeafNode<T, const B: usize, const C: usize> {
    pub base: NodeBase<T, B, C>,
    values: [MaybeUninit<T>; C],
}

impl<T, const B: usize, const C: usize> NodeBase<T, B, C> {
    pub const fn new() -> Self {
        Self {
            parent: None,
            parent_index: MaybeUninit::uninit(),
            children_len: MaybeUninit::new(0),
            _marker: PhantomData,
        }
    }
}

impl<T, const B: usize, const C: usize> LeafNode<T, B, C> {
    const UNINIT_T: MaybeUninit<T> = MaybeUninit::uninit();

    pub fn new() -> NonNull<Self> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            values: [Self::UNINIT_T; C],
        })))
    }

    pub fn from_value(value: T) -> NonNull<Self> {
        let mut leaf = Self::new();
        unsafe {
            leaf.as_mut().values.as_mut()[0].write(value);
            leaf.as_mut().base.children_len.write(1);
        };
        leaf
    }
}

impl<T, const B: usize, const C: usize> InternalNode<T, B, C> {
    const UNINIT_NODE: MaybeUninit<NonNull<NodeBase<T, B, C>>> = MaybeUninit::uninit();

    pub fn new() -> NonNull<Self> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            lengths: [MaybeUninit::uninit(); B],
            children: [Self::UNINIT_NODE; B],
        })))
    }

    pub fn from_child_array<const N: usize>(
        children: [(usize, NodePtr<T, B, C>); N],
    ) -> NonNull<Self> {
        let boxed_children = Self::new();
        let mut vec = unsafe {
            handle::Node::<handle::ownership::Mut, _, _, B, C>::new_internal(boxed_children.cast())
                .as_array_vec()
        };
        for (i, (child_len, mut child)) in children.into_iter().enumerate() {
            unsafe {
                child.as_mut().parent = Some(boxed_children.cast());
                child.as_mut().parent_index.write(i as u16);
                (*boxed_children.as_ptr()).lengths[i].write(child_len);
            }
            vec.push_back(child);
        }

        boxed_children
    }
}
