use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

use crate::utils::{slice_assume_init_ref, ArrayVecMut};

// use crate::panics::panic_length_overflow;

pub mod handle;

pub struct Node<T, const B: usize, const C: usize> {
    pub length: usize,
    pub ptr: NonNull<NodeBase<T, B, C>>,
}

pub struct NodeBase<T, const B: usize, const C: usize> {
    pub parent: Option<NonNull<InternalNode<T, B, C>>>,
    pub parent_index: MaybeUninit<u16>,
    pub children_len: MaybeUninit<u16>,
    _marker: PhantomData<T>,
}

#[repr(C)]
pub struct InternalNode<T, const B: usize, const C: usize> {
    base: NodeBase<T, B, C>,
    pub children: [MaybeUninit<Node<T, B, C>>; B],
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

    pub const fn new() -> Self {
        Self {
            base: NodeBase::new(),
            values: [Self::UNINIT_T; C],
        }
    }
}

impl<T, const B: usize, const C: usize> InternalNode<T, B, C> {
    const UNINIT_NODE: MaybeUninit<Node<T, B, C>> = MaybeUninit::uninit();

    pub const fn new() -> Self {
        Self {
            base: NodeBase::new(),
            children: [Self::UNINIT_NODE; B],
        }
    }

    pub fn children(&self) -> &[Node<T, B, C>] {
        unsafe {
            slice_assume_init_ref(
                self.children
                    .get_unchecked(..usize::from(self.base.children_len.assume_init())),
            )
        }
    }

    pub fn as_array_vec(&mut self) -> ArrayVecMut<Node<T, B, C>, B> {
        unsafe { ArrayVecMut::new(&mut self.children, self.base.children_len.assume_init_mut()) }
    }

    pub fn sum_lens(&self) -> usize {
        self.children().iter().map(Node::len).sum()
    }
}

impl<T, const B: usize, const C: usize> Node<T, B, C> {
    #[inline]
    pub const fn len(&self) -> usize {
        self.length
    }

    pub fn from_child_array<const N: usize>(children: [Self; N]) -> Self {
        let mut boxed_children = Box::new(InternalNode::new());
        let mut vec = boxed_children.as_array_vec();
        let ptr = NonNull::from(Box::leak(boxed_children));
        let mut length = 0;
        for (i, mut child) in children.into_iter().enumerate() {
            length += child.len();
            unsafe {
                child.ptr.as_mut().parent = Some(ptr);
                child.ptr.as_mut().parent_index.write(i as u16);
            }
            vec.push_back(child);
        }

        Self {
            length,
            ptr: ptr.cast::<NodeBase<T, B, C>>(),
        }
    }

    fn empty_leaf() -> Self {
        let values = NonNull::from(Box::leak(Box::new(LeafNode::<T, B, C>::new())));
        Self {
            length: 0,
            ptr: values.cast::<NodeBase<T, B, C>>(),
        }
    }

    pub fn from_value(value: T) -> Self {
        let mut leaf = Self::empty_leaf();
        leaf.length = 1;
        unsafe {
            leaf.ptr
                .cast::<LeafNode<T, B, C>>()
                .as_mut()
                .values
                .as_mut()[0]
                .write(value);
            leaf.ptr.as_mut().children_len.write(1);
        };
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
