use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use alloc::boxed::Box;

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
    length: NonZeroUsize,
    pub ptr: NodePtr<T, B, C>,
}

pub union NodePtr<T, const B: usize, const C: usize> {
    pub children: NonNull<[Option<Node<T, B, C>>; B]>,
    values: NonNull<[MaybeUninit<T>; C]>,
}

impl<T, const B: usize, const C: usize> Node<T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();
    const NONE: Option<Self> = None;

    #[inline]
    pub const fn len(&self) -> usize {
        self.length.get()
    }

    fn from_children(length: usize, children: Box<[Option<Self>; B]>) -> Self {
        Self {
            length: NonZeroUsize::new(length).unwrap(),
            ptr: NodePtr {
                children: unsafe { NonNull::new_unchecked(Box::into_raw(children)) },
            },
        }
    }

    pub fn from_child_array<const N: usize>(children: [Self; N]) -> Self {
        let mut boxed_children = Box::new([Self::NONE; B]);
        let mut length = 0;
        for (i, child) in children.into_iter().enumerate() {
            length += child.len();
            boxed_children[i] = Some(child);
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
            length: NonZeroUsize::new(length).unwrap(),
            ptr: NodePtr {
                values: unsafe { NonNull::new_unchecked(Box::into_raw(values)) },
            },
        }
    }

    pub fn from_value(value: T) -> Self {
        let mut boxed_values = Box::new([Self::UNINIT; C]);
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

        assert_eq!(size_of::<Node<i32, 10, 37>>(), 2 * size_of::<usize>());
        assert_eq!(size_of::<Node<i128, 3, 3>>(), 2 * size_of::<usize>());
        assert_eq!(size_of::<Node<(), 3, 3>>(), 2 * size_of::<usize>());

        assert_eq!(
            size_of::<Option<Node<i32, 10, 37>>>(),
            2 * size_of::<usize>()
        );
        assert_eq!(
            size_of::<Option<Node<i128, 3, 3>>>(),
            2 * size_of::<usize>()
        );
        assert_eq!(size_of::<Option<Node<(), 3, 3>>>(), 2 * size_of::<usize>());
    }
}
