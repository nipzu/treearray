mod fenwick;
pub mod handle;

use core::{
    alloc::Layout,
    marker::PhantomData,
    mem::{size_of, ManuallyDrop, MaybeUninit},
    ptr::NonNull,
};

use alloc::{
    alloc::{alloc, handle_alloc_error},
    boxed::Box,
};

use self::fenwick::FenwickTree;

/// SAFETY: BRANCH_FACTOR must be less than u8::MAX.
#[cfg(test)]
const BRANCH_FACTOR: usize = 4;
#[cfg(not(test))]
const BRANCH_FACTOR: usize = 32;

/// SAFETY: LEAF_CAP_BYTES must be less than u16::MAX.
#[cfg(test)]
const LEAF_CAP_BYTES: usize = 16;
#[cfg(not(test))]
const LEAF_CAP_BYTES: usize = 256;

pub struct RawNodeWithLen<T>(pub usize, pub NodePtr<T>);

pub union NodePtr<T> {
    internal: ManuallyDrop<Box<InternalNode<T>>>,
    pub leaf: NonNull<LeafBase<T>>,
}

impl<T> NodePtr<T> {
    pub unsafe fn internal_ref(&self) -> &InternalNode<T> {
        unsafe { &self.internal }
    }

    pub unsafe fn internal_mut(&mut self) -> &mut InternalNode<T> {
        unsafe { &mut self.internal }
    }

    pub unsafe fn into_internal(self) -> Box<InternalNode<T>> {
        unsafe { ManuallyDrop::into_inner(self.internal) }
    }
}

pub struct NodeBase<T> {
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T> {
    pub children_len: u16,
    pub lengths: FenwickTree,
    pub children: [MaybeUninit<NodePtr<T>>; BRANCH_FACTOR],
}

#[repr(C)]
pub struct LeafBase<T> {
    pub next: Option<NonNull<LeafBase<T>>>,
    prev: Option<NonNull<LeafBase<T>>>,
    pub len: u16,
    _p: PhantomData<T>,
}

impl<T> LeafBase<T> {
    pub fn new() -> Self {
        Self {
            next: None,
            prev: None,
            len: 0,
            _p: PhantomData,
        }
    }
}

// #[repr(C)]
// pub struct LeafNode<T, > {
//     base: LeafBase<T>,
//     values: [MaybeUninit<T>; C],
// }

impl<T> NodeBase<T> {
    // TODO: does this have to be even (for split)
    const LEAF_CAP: usize = 2 * if size_of::<T>() <= LEAF_CAP_BYTES {
        if size_of::<T>() == 0 {
            // TODO: what should this be?
            1
        } else {
            LEAF_CAP_BYTES / size_of::<T>()
        }
    } else {
        1
    };

    pub fn new_leaf() -> NodePtr<T> {
        let (layout, _) = Self::leaf_layout();
        let ptr = unsafe { alloc(layout).cast::<LeafBase<T>>() };
        let Some(node_ptr) = NonNull::new(ptr) else {
            handle_alloc_error(layout);
        };
        unsafe { node_ptr.as_ptr().write(LeafBase::new()) };
        NodePtr { leaf: node_ptr }
    }

    pub fn leaf_layout() -> (Layout, usize) {
        let base = Layout::new::<LeafBase<T>>();
        let array = Layout::array::<T>(Self::LEAF_CAP).unwrap();
        let (layout, offset) = base.extend(array).unwrap();
        // Remember to finalize with `pad_to_align`!
        (layout.pad_to_align(), offset)
    }
}

impl<T> InternalNode<T> {
    const UNINIT_NODE: MaybeUninit<NodePtr<T>> = MaybeUninit::uninit();

    pub fn new() -> NodePtr<T> {
        NodePtr {
            internal: ManuallyDrop::new(Box::new(Self {
                children_len: 0,
                lengths: FenwickTree::new(),
                children: [Self::UNINIT_NODE; BRANCH_FACTOR],
            })),
        }
    }

    pub fn from_child_array<const N: usize>(children: [RawNodeWithLen<T>; N]) -> NodePtr<T> {
        let mut boxed_children = Self::new();
        let children_mut = unsafe { boxed_children.internal_mut() };
        for child in children {
            unsafe { children_mut.push_back_child(child) }
        }

        boxed_children
    }
}
