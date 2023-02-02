mod fenwick;
pub mod handle;

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

use self::{fenwick::FenwickTree, handle::InternalMut};

/// SAFETY: BRANCH_FACTOR must be less than u8::MAX.
#[cfg(miri)]
const BRANCH_FACTOR: usize = 4;
#[cfg(not(miri))]
const BRANCH_FACTOR: usize = 32;

/// SAFETY: LEAF_CAP_BYTES must be less than u16::MAX.
#[cfg(miri)]
const LEAF_CAP_BYTES: usize = 16;
#[cfg(not(miri))]
const LEAF_CAP_BYTES: usize = 256;

pub struct RawNodeWithLen<T>(pub usize, pub NodePtr<T>);

pub type NodePtr<T> = NonNull<NodeBase<T>>;

pub struct NodeBase<T> {
    children_len: u16,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T> {
    base: NodeBase<T>,
    pub lengths: FenwickTree,
    pub children: [MaybeUninit<NodePtr<T>>; BRANCH_FACTOR],
}

#[repr(C)]
pub struct LeafBase {
    next: Option<NonNull<LeafBase>>,
    prev: Option<NonNull<LeafBase>>,
    len: u16,
}

impl LeafBase {
    pub fn new() -> Self {
        Self {
            next: None,
            prev: None,
            len: 0,
        }
    }
}

// #[repr(C)]
// pub struct LeafNode<T, > {
//     base: LeafBase<T>,
//     values: [MaybeUninit<T>; C],
// }

impl<T> NodeBase<T> {
    pub const fn new() -> Self {
        Self {
            children_len: 0,
            _marker: PhantomData,
        }
    }
}

impl<T> NodeBase<T> {
    const LEAF_CAP: usize = if size_of::<T>() <= LEAF_CAP_BYTES {
        if size_of::<T>() == 0 {
            // TODO: should this be 0?
            1
        } else {
            LEAF_CAP_BYTES / size_of::<T>()
        }
    } else {
        1
    };

    pub fn new_leaf() -> NodePtr<T> {
        let (layout, _) = Self::leaf_layout();
        let ptr = unsafe { alloc(layout).cast::<LeafBase>() };
        let Some(node_ptr) = NonNull::new(ptr) else {
            handle_alloc_error(layout);
        };
        unsafe { node_ptr.as_ptr().write(LeafBase::new()) };
        node_ptr.cast()
    }

    pub fn leaf_layout() -> (Layout, usize) {
        let base = Layout::new::<LeafBase>();
        let array = Layout::array::<T>(Self::LEAF_CAP).unwrap();
        let (layout, offset) = base.extend(array).unwrap();
        // Remember to finalize with `pad_to_align`!
        (layout.pad_to_align(), offset)
    }
}

impl<T> InternalNode<T> {
    const UNINIT_NODE: MaybeUninit<NodePtr<T>> = MaybeUninit::uninit();

    pub fn new() -> NodePtr<T> {
        NonNull::from(Box::leak(Box::new(Self {
            base: NodeBase::new(),
            lengths: FenwickTree::new(),
            children: [Self::UNINIT_NODE; BRANCH_FACTOR],
        })))
        .cast()
    }

    pub fn from_child_array<const N: usize>(children: [RawNodeWithLen<T>; N]) -> NodePtr<T> {
        let boxed_children = Self::new();
        let mut children_mut = unsafe { InternalMut::new(boxed_children) };
        for child in children {
            unsafe { children_mut.push_back_child(child) }
        }

        boxed_children
    }
}
