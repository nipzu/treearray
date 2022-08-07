use core::{
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

use alloc::boxed::Box;

use crate::utils::{slice_assume_init_mut, slice_assume_init_ref, slice_shift_right};

// use crate::panics::panic_length_overflow;

pub mod handle;

pub struct Node<T, const B: usize, const C: usize> {
    // INVARIANT: `length` is the number of values that this node eventually has as children
    //
    // If `self.length <= C`, this node is a leaf node with
    // exactly `size` initialized values held in `self.ptr.values`.
    // TODO: > C/2 when not root
    //
    // If `self.length > C`, this node is an internal node. Under normal
    // conditions, there should be some n such that the first n children
    // are `Some`, and the sum of their `len()`s is equal to `self.len()`.
    // Normal logic can assume that this assumption is upheld.
    // However, breaking this assumption must not cause Undefined Behavior.
    length: usize,
    ptr: NodePtr<T, B, C>,
}

// `Box`es cannot be used here because they would assert
// unique ownership over the child node. We don't want that
// because aliasing pointers are created when using `CursorMut`.
union NodePtr<T, const B: usize, const C: usize> {
    children: NonNull<Children<T, B, C>>,
    values: NonNull<[MaybeUninit<T>; C]>,
}

pub struct Children<T, const B: usize, const C: usize> {
    children: [MaybeUninit<Node<T, B, C>>; B],
    len: usize,
}

impl<T, const B: usize, const C: usize> Children<T, B, C> {
    const UNINIT_NODE: MaybeUninit<Node<T, B, C>> = MaybeUninit::uninit();

    pub const fn new() -> Self {
        Self {
            children: [Self::UNINIT_NODE; B],
            len: 0,
        }
    }

    pub fn children(&self) -> &[Node<T, B, C>] {
        unsafe { slice_assume_init_ref(self.children.get_unchecked(..self.len)) }
    }

    pub fn children_mut(&mut self) -> &mut [Node<T, B, C>] {
        unsafe { slice_assume_init_mut(self.children.get_unchecked_mut(..self.len)) }
    }

    pub fn pair_at(&mut self, index: usize) -> (&mut Node<T, B, C>, &mut Node<T, B, C>) {
        if let [ref mut fst, ref mut snd, ..] = self.children_mut()[index..] {
            (fst, snd)
        } else {
            unreachable!()
        }
    }

    pub fn insert(&mut self, index: usize, value: Node<T, B, C>) {
        assert!(self.len < B);
        assert!(index <= self.len);
        self.len += 1;
        slice_shift_right(&mut self.children[index..self.len], MaybeUninit::new(value));
    }

    pub fn split(&mut self, index: usize) -> Box<Self> {
        assert!(self.len <= B);
        assert!(index <= self.len);
        let mut new_children = Box::new(Self::new());
        // use B insted of self.len
        // self.len should be B or B - 1
        new_children.len = self.len - index;
        self.len = index;
        unsafe {
            ptr::copy_nonoverlapping(
                self.children.as_ptr().add(index),
                new_children.children.as_mut_ptr(),
                B - index,
            );
        }
        new_children
    }

    pub fn merge_with_next(&mut self, next: &mut Self) {
        assert!(self.len + next.len <= B);
        unsafe {
            ptr::copy_nonoverlapping(
                next.children.as_ptr(),
                self.children.as_mut_ptr().add(self.len),
                next.len,
            );
        }
        self.len += next.len;
        next.len = 0;
    }

    pub fn sum_lens(&self) -> usize {
        self.children().iter().map(Node::len).sum()
    }
}

impl<T, const B: usize, const C: usize> Node<T, B, C> {
    const UNINIT_T: MaybeUninit<T> = MaybeUninit::uninit();
    const UNINIT_SELF: MaybeUninit<Self> = MaybeUninit::uninit();

    #[inline]
    pub const fn len(&self) -> usize {
        self.length
    }

    fn from_children(length: usize, children: Box<Children<T, B, C>>) -> Self {
        debug_assert_eq!(children.sum_lens(), length);
        Self {
            length,
            ptr: NodePtr {
                children: NonNull::from(Box::leak(children)),
            },
        }
    }

    pub fn from_child_array<const N: usize>(children: [Self; N]) -> Self {
        let mut boxed_children = Box::new(Children {
            children: [Self::UNINIT_SELF; B],
            len: 0,
        });
        let mut length = 0;
        for (i, child) in children.into_iter().enumerate() {
            length += child.len();
            boxed_children.len += 1;
            boxed_children.children[i].write(child);
        }

        Self::from_children(length, boxed_children)
    }

    /// # Safety
    ///
    /// The first `length` elements of `values` must be initialized and safe to use.
    /// Note that this implies that `length <= C`.
    unsafe fn from_values(length: usize, values: Box<[MaybeUninit<T>; C]>) -> Self {
        assert!(length <= C);

        // SAFETY: `length <= C`, so we return a leaf node
        // which has the same safety invariants as this function
        Self {
            length,
            ptr: NodePtr {
                values: NonNull::from(Box::leak(values)),
            },
        }
    }

    pub fn from_value(value: T) -> Self {
        let mut boxed_values = Box::new([Self::UNINIT_T; C]);
        boxed_values[0].write(value);
        unsafe { Self::from_values(1, boxed_values) }
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
