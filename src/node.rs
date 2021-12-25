use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::num::NonZeroUsize;
use core::ptr;

use alloc::boxed::Box;

use crate::panics::panic_length_overflow;

pub mod handle;

use handle::{InternalHandle, InternalHandleMut, LeafHandle, LeafHandleMut};

pub struct Node<T, const B: usize, const C: usize> {
    // INVARIANT: `length` is the number of values that this node eventually has as children
    //
    // If `self.length <= C`, this node is a leaf node with
    // exactly `size` initialized values held in `self.inner.values`.
    // TODO: > C/2 when not root
    //
    // If `self.length > C`, this node is an internal node. Under normal
    // conditions, there should be some n such that the first n children
    // are `Some`, and the sum of their `len()`s is equal to `self.len()`.
    // Normal logic can assume that this assumption is upheld.
    // However, breaking this assumption must not cause Undefined Behavior.
    length: NonZeroUsize,
    inner: NodeInner<T, B, C>,
}

union NodeInner<T, const B: usize, const C: usize> {
    children: ManuallyDrop<Box<[Option<Node<T, B, C>>; B]>>,
    values: ManuallyDrop<Box<[MaybeUninit<T>; C]>>,
}

pub enum NodeVariant<'a, T, const B: usize, const C: usize> {
    Internal { handle: InternalHandle<'a, T, B, C> },
    Leaf { handle: LeafHandle<'a, T, B, C> },
}

pub enum NodeVariantMut<'a, T, const B: usize, const C: usize> {
    Internal {
        handle: InternalHandleMut<'a, T, B, C>,
    },
    Leaf {
        handle: LeafHandleMut<'a, T, B, C>,
    },
}

impl<T, const B: usize, const C: usize> Node<T, B, C> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();
    const NONE: Option<Self> = None;

    #[inline]
    pub const fn len(&self) -> usize {
        self.length.get()
    }

    pub fn free(mut self) {
        match self.variant_mut() {
            NodeVariantMut::Leaf { .. } => unsafe {
                ManuallyDrop::drop(&mut self.inner.values);
            },
            NodeVariantMut::Internal { .. } => unsafe {
                ManuallyDrop::drop(&mut self.inner.children);
            },
        }
        mem::forget(self);
    }

    fn is_full(&self) -> bool {
        match self.variant() {
            NodeVariant::Leaf { handle } => handle.values().len() == C, // TODO >= for compiler hints?
            NodeVariant::Internal { handle } => matches!(handle.children().last(), Some(&Some(_))),
        }
    }

    fn from_children(length: usize, children: Box<[Option<Self>; B]>) -> Self {
        assert!(length > C);

        // SAFETY: `length > C`, so the `Node` is considered an internal node as it should be.
        Self {
            length: NonZeroUsize::new(length).unwrap(),
            inner: NodeInner {
                children: ManuallyDrop::new(children),
            },
        }
    }

    pub fn from_child_array<const N: usize>(children: [Self; N]) -> Self {
        let mut inner_children = [Self::NONE; B];
        let mut length = 0;
        for (i, child) in children.into_iter().enumerate() {
            length += child.len();
            inner_children[i] = Some(child);
        }

        Self::from_children(length, Box::new(inner_children))
    }

    /// # Safety
    ///
    /// The first `length` elements of `values` must be initialized and safe to use.
    /// Note that this implies the condition `length <= C`.
    unsafe fn from_values(length: usize, values: Box<[MaybeUninit<T>; C]>) -> Self {
        assert!(length <= C);

        // SAFETY: `length <= C`, so we return a leaf node
        // which has the same safety invariants as this function
        Self {
            length: NonZeroUsize::new(length).unwrap(),
            inner: NodeInner {
                values: ManuallyDrop::new(values),
            },
        }
    }

    pub fn from_value(value: T) -> Self {
        let mut boxed_values = Box::new([Self::UNINIT; C]);
        boxed_values[0].write(value);

        // SAFETY: the first value has been written to, so it is initialized.
        // Since `1 <= C` by the const invariants of `BTreeVec`, the `Node` is considered
        // a leaf node and the first value is initialized, satisfying `length = 1`.
        Self {
            length: NonZeroUsize::new(1).unwrap(),
            inner: NodeInner {
                values: ManuallyDrop::new(boxed_values),
            },
        }
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Self> {
        match self.variant_mut() {
            NodeVariantMut::Internal { mut handle } => handle.insert(index, value),
            NodeVariantMut::Leaf { mut handle } => handle.insert(index, value),
        }
    }

    pub const fn variant(&self) -> NodeVariant<T, B, C> {
        if self.len() <= C {
            NodeVariant::Leaf {
                // SAFETY: the safety invariant `self.len() <= C` is satisfied.
                handle: unsafe { LeafHandle::new(self) },
            }
        } else {
            NodeVariant::Internal {
                // SAFETY: the safety invariant `self.len() > C` is satisfied.
                handle: unsafe { InternalHandle::new(self) },
            }
        }
    }

    pub fn variant_mut(&mut self) -> NodeVariantMut<T, B, C> {
        if self.len() <= C {
            NodeVariantMut::Leaf {
                // SAFETY: the safety invariant `self.len() <= C` is satisfied.
                handle: unsafe { LeafHandleMut::new(self) },
            }
        } else {
            NodeVariantMut::Internal {
                // SAFETY: the safety invariant `self.len() > C` is satisfied.
                handle: unsafe { InternalHandleMut::new(self) },
            }
        }
    }
}

impl<T, const B: usize, const C: usize> Drop for Node<T, B, C> {
    fn drop(&mut self) {
        match self.variant_mut() {
            NodeVariantMut::Leaf { mut handle } => unsafe {
                // Drop the values of a leaf node and deallocate afterwards.

                // SAFETY: `values_mut` returns a properly aligned slice that is valid for both
                // reads and writes. The contents of the slice are properly initialized values
                // of type `T``and such be valid for dropping as such.
                // This is a drop method which will be called at most once, which means that
                // the values will also get dropped at most once.
                ptr::drop_in_place(handle.values_mut());

                // SAFETY: This node is a leaf node, so`self.inner.values` can be accessed.
                // This is a drop method which will be called at most once, which means that
                // `self.inner.values` will also get dropped at most once.
                ManuallyDrop::drop(&mut self.inner.values);
            },
            NodeVariantMut::Internal { .. } => unsafe {
                // SAFETY: This node is a leaf node, so`self.inner.children` can be accessed.
                // This is a drop method which will be called at most once, which means that
                // `self.inner.children` will also get dropped at most once.
                ManuallyDrop::drop(&mut self.inner.children);
            },
        }
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
