use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

use self::handle::InternalMut;

// use crate::panics::panic_length_overflow;

pub mod handle;

pub struct RawNodeWithLen<T, const B: usize, const C: usize>(pub usize, pub NodePtr<T, B, C>);

pub type NodePtr<T, const B: usize, const C: usize> = NonNull<NodeBase<T, B, C>>;

pub struct NodeBase<T, const B: usize, const C: usize> {
    pub parent: Option<NodePtr<T, B, C>>,
    pub parent_index: MaybeUninit<u16>,
    pub children_len: u16,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T, const B: usize, const C: usize> {
    base: NodeBase<T, B, C>,
    pub children: [MaybeUninit<NodePtr<T, B, C>>; B],
    pub lengths: [MaybeUninit<usize>; B],
}

#[repr(C)]
pub struct LeafNode<T, const B: usize, const C: usize> {
    base: NodeBase<T, B, C>,
    values: [MaybeUninit<T>; C],
}

impl<T, const B: usize, const C: usize> NodeBase<T, B, C> {
    pub const fn new() -> Self {
        Self {
            parent: None,
            parent_index: MaybeUninit::uninit(),
            children_len: 0,
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
            leaf.as_mut().base.children_len = 1;
        };
        leaf
    }
}

impl<T, const B: usize, const C: usize> InternalNode<T, B, C> {
    const UNINIT_NODE: MaybeUninit<NodePtr<T, B, C>> = MaybeUninit::uninit();

    pub fn new() -> NonNull<Self> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            lengths: [MaybeUninit::uninit(); B],
            children: [Self::UNINIT_NODE; B],
        })))
    }

    pub fn from_child_array<const N: usize>(
        children: [RawNodeWithLen<T, B, C>; N],
    ) -> NonNull<Self> {
        let boxed_children = Self::new();
        let mut vec = unsafe { InternalMut::<T, B, C>::new(boxed_children.cast()).children() };
        for (i, RawNodeWithLen(child_len, mut child)) in children.into_iter().enumerate() {
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
