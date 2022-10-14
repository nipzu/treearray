use core::{
    alloc::Layout,
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
    ptr::NonNull,
};

use alloc::{
    alloc::{alloc, handle_alloc_error},
    boxed::Box,
};

use self::handle::InternalMut;

// use crate::panics::panic_length_overflow;

#[cfg(miri)]
const BRANCH_FACTOR: usize = 4;
#[cfg(not(miri))]
const BRANCH_FACTOR: usize = 32;

#[cfg(miri)]
const LEAF_CAP_BYTES: usize = 16;
#[cfg(not(miri))]
const LEAF_CAP_BYTES: usize = 256;

pub mod handle;

pub struct RawNodeWithLen<T>(pub usize, pub NodePtr<T>);

pub type NodePtr<T> = NonNull<NodeBase<T>>;

pub struct NodeBase<T> {
    pub parent: Option<NodePtr<T>>,
    pub parent_index: MaybeUninit<u16>,
    pub children_len: u16,
    pub height: u16,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T> {
    base: NodeBase<T>,
    pub lengths: [usize; BRANCH_FACTOR],
    pub children: [MaybeUninit<NodePtr<T>>; BRANCH_FACTOR],
}

// #[repr(C)]
// pub struct LeafNode<T, > {
//     base: NodeBase<T>,
//     values: [MaybeUninit<T>; C],
// }

impl<T> NodeBase<T> {
    pub const fn new(height: u16) -> Self {
        Self {
            parent: None,
            parent_index: MaybeUninit::uninit(),
            children_len: 0,
            height,
            _marker: PhantomData,
        }
    }
}

impl<T> NodeBase<T> {
    const LEAF_CAP: usize = if size_of::<T>() <= LEAF_CAP_BYTES {
        LEAF_CAP_BYTES / size_of::<T>()
    } else {
        1
    };

    pub fn new_leaf() -> NodePtr<T> {
        let (layout, _) = Self::leaf_layout();
        let ptr = unsafe { alloc(layout).cast::<NodeBase<T>>() };
        let Some(node_ptr) = NonNull::new(ptr) else {
            handle_alloc_error(layout);
        };
        unsafe { node_ptr.as_ptr().write(NodeBase::new(0)) };
        node_ptr
    }

    pub fn leaf_layout() -> (Layout, usize) {
        let base = Layout::new::<NodeBase<T>>();
        let array = Layout::array::<T>(Self::LEAF_CAP).unwrap();
        let (layout, offset) = base.extend(array).unwrap();
        // Remember to finalize with `pad_to_align`!
        (layout.pad_to_align(), offset)
    }
}

impl<T> InternalNode<T> {
    const UNINIT_NODE: MaybeUninit<NodePtr<T>> = MaybeUninit::uninit();

    pub fn new(height: u16) -> NodePtr<T> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(height),
            lengths: [0; BRANCH_FACTOR],
            children: [Self::UNINIT_NODE; BRANCH_FACTOR],
        })))
        .cast()
    }

    pub fn from_child_array<const N: usize>(children: [RawNodeWithLen<T>; N]) -> NodePtr<T> {
        let height = unsafe { children[0].1.as_ref().height + 1 };
        let boxed_children = Self::new(height);
        let mut children_mut = unsafe { InternalMut::new(boxed_children) };
        for child in children {
            unsafe { children_mut.push_back_child(child) }
        }

        boxed_children
    }
}
