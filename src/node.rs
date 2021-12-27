use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::num::NonZeroUsize;
use core::ptr;

use alloc::boxed::Box;

// use crate::panics::panic_length_overflow;

pub mod handle;

use handle::{Internal, InternalMut, Leaf, LeafMut};

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
    ptr: NodePtr<T, B, C>,
}

union NodePtr<T, const B: usize, const C: usize> {
    children: ManuallyDrop<Box<[Option<Node<T, B, C>>; B]>>,
    values: ManuallyDrop<Box<[MaybeUninit<T>; C]>>,
}

pub enum Variant<'a, T, const B: usize, const C: usize> {
    Internal { handle: Internal<'a, T, B, C> },
    Leaf { handle: Leaf<'a, T, B, C> },
}

pub enum VariantMut<'a, T, const B: usize, const C: usize> {
    Internal { handle: InternalMut<'a, T, B, C> },
    Leaf { handle: LeafMut<'a, T, B, C> },
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
            VariantMut::Leaf { .. } => unsafe {
                ManuallyDrop::drop(&mut self.ptr.values);
            },
            VariantMut::Internal { .. } => unsafe {
                ManuallyDrop::drop(&mut self.ptr.children);
            },
        }
        mem::forget(self);
    }

    fn is_full(&self) -> bool {
        match self.variant() {
            Variant::Leaf { handle } => handle.values().len() == C,
            Variant::Internal { handle } => matches!(handle.children().last(), Some(&Some(_))),
        }
    }

    fn from_children(length: usize, children: Box<[Option<Self>; B]>) -> Self {
        assert!(length > C);

        // SAFETY: `length > C`, so the `Node` is considered an internal node as it should be.
        Self {
            length: NonZeroUsize::new(length).unwrap(),
            ptr: NodePtr {
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
            ptr: NodePtr {
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
            ptr: NodePtr {
                values: ManuallyDrop::new(boxed_values),
            },
        }
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Self> {
        match self.variant_mut() {
            VariantMut::Internal { mut handle } => handle.insert(index, value),
            VariantMut::Leaf { mut handle } => handle.insert(index, value),
        }
    }

    pub const fn variant(&self) -> Variant<T, B, C> {
        if self.len() <= C {
            Variant::Leaf {
                // SAFETY: the safety invariant `self.len() <= C` is satisfied.
                handle: unsafe { Leaf::new(self) },
            }
        } else {
            Variant::Internal {
                // SAFETY: the safety invariant `self.len() > C` is satisfied.
                handle: unsafe { Internal::new(self) },
            }
        }
    }

    pub fn variant_mut(&mut self) -> VariantMut<T, B, C> {
        if self.len() <= C {
            VariantMut::Leaf {
                // SAFETY: the safety invariant `self.len() <= C` is satisfied.
                handle: unsafe { LeafMut::new(self) },
            }
        } else {
            VariantMut::Internal {
                // SAFETY: the safety invariant `self.len() > C` is satisfied.
                handle: unsafe { InternalMut::new(self) },
            }
        }
    }
}

impl<T, const B: usize, const C: usize> Drop for Node<T, B, C> {
    fn drop(&mut self) {
        match self.variant_mut() {
            VariantMut::Leaf { mut handle } => unsafe {
                // Drop the values of a leaf node and deallocate afterwards.

                // SAFETY: `values_mut` returns a properly aligned slice that is valid for both
                // reads and writes. The contents of the slice are properly initialized values
                // of type `T``and such be valid for dropping as such.
                // This is a drop method which will be called at most once, which means that
                // the values will also get dropped at most once.
                ptr::drop_in_place(handle.values_mut());

                // SAFETY: This node is a leaf node, so `self.ptr.values` can be accessed.
                // This is a drop method which will be called at most once, which means that
                // `self.ptr.values` will also get dropped at most once.
                ManuallyDrop::drop(&mut self.ptr.values);
            },
            VariantMut::Internal { .. } => unsafe {
                // SAFETY: This node is a leaf node, so `self.ptr.children` can be accessed.
                // This is a drop method which will be called at most once, which means that
                // `self.ptr.children` will also get dropped at most once.
                ManuallyDrop::drop(&mut self.ptr.children);
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
