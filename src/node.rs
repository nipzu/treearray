use core::{mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

use crate::utils::{slice_assume_init_ref, ArrayVecMut};

// use crate::panics::panic_length_overflow;

pub mod handle;

pub struct Node<T, const B: usize, const C: usize> {
    pub length: usize,
    pub ptr: NodePtr<T, B, C>,
}

// `Box`es cannot be used here because they would assert
// unique ownership over the child node. We don't want that
// because aliasing pointers are created when using `CursorMut`.
pub union NodePtr<T, const B: usize, const C: usize> {
    pub children: NonNull<InternalNode<T, B, C>>,
    values: NonNull<[MaybeUninit<T>; C]>,
}

pub struct InternalNode<T, const B: usize, const C: usize> {
    pub children: [MaybeUninit<Node<T, B, C>>; B],
    len: usize,
    parent_node: Option<NonNull<Self>>,
    parent_index: MaybeUninit<usize>,
}

impl<T, const B: usize, const C: usize> InternalNode<T, B, C> {
    const UNINIT_NODE: MaybeUninit<Node<T, B, C>> = MaybeUninit::uninit();

    pub const fn new() -> Self {
        Self {
            children: [Self::UNINIT_NODE; B],
            len: 0,
            parent_node: None,
            parent_index: MaybeUninit::uninit(),
        }
    }

    pub fn children(&self) -> &[Node<T, B, C>] {
        unsafe { slice_assume_init_ref(self.children.get_unchecked(..self.len)) }
    }

    pub fn as_array_vec(&mut self) -> ArrayVecMut<Node<T, B, C>, usize, B> {
        unsafe { ArrayVecMut::new(&mut self.children, &mut self.len) }
    }

    pub fn sum_lens(&self) -> usize {
        self.children().iter().map(Node::len).sum()
    }
}

impl<T, const B: usize, const C: usize> Node<T, B, C> {
    const UNINIT_T: MaybeUninit<T> = MaybeUninit::uninit();

    #[inline]
    pub const fn len(&self) -> usize {
        self.length
    }

    unsafe fn from_children(length: usize, children: Box<InternalNode<T, B, C>>) -> Self {
        debug_assert_eq!(children.sum_lens(), length);
        Self {
            length,
            ptr: NodePtr {
                children: NonNull::from(Box::leak(children)),
            },
        }
    }

    pub fn from_child_array<const N: usize>(children: [Self; N]) -> Self {
        let mut boxed_children = Box::new(InternalNode::new());
        let mut length = 0;
        for child in children {
            length += child.len();
            boxed_children.as_array_vec().push_back(child);
        }

        unsafe { Self::from_children(length, boxed_children) }
    }

    fn empty_leaf() -> Self {
        let values = NonNull::from(Box::leak(Box::new([Self::UNINIT_T; C])));
        Self {
            length: 0,
            ptr: NodePtr { values },
        }
    }

    pub fn from_value(value: T) -> Self {
        let mut leaf = Self::empty_leaf();
        leaf.length = 1;
        unsafe { leaf.ptr.values.as_mut()[0].write(value) };
        leaf
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_node_size() {
        use core::mem::size_of;

        let node_size = size_of::<usize>() + size_of::<*mut ()>();

        assert_eq!(size_of::<Node<i32, 10, 37>>(), node_size);
        assert_eq!(size_of::<Node<i128, 3, 3>>(), node_size);
        assert_eq!(size_of::<Node<(), 3, 3>>(), node_size);

        assert_eq!(size_of::<MaybeUninit<Node<i32, 10, 37>>>(), node_size);
        assert_eq!(size_of::<MaybeUninit<Node<i128, 3, 3>>>(), node_size);
        assert_eq!(size_of::<MaybeUninit<Node<(), 3, 3>>>(), node_size);
    }
}
