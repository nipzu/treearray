use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use alloc::boxed::Box;

// use crate::panics::panic_length_overflow;

pub mod handle;

use handle::{Internal, InternalMut, Leaf, LeafMut};

pub struct DynNode<'a, T, const B: usize, const C: usize> {
    height: usize,
    node: &'a Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> DynNode<'a, T, B, C> {
    pub const unsafe fn new(height: usize, node: &'a Node<T, B, C>) -> Self {
        Self { height, node }
    }

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub fn variant(&self) -> Variant<'a, T, B, C> {
        if self.height == 0 {
            Variant::Leaf {
                // TODO:
                // SAFETY:
                handle: unsafe { Leaf::new(self.node) },
            }
        } else {
            Variant::Internal {
                // TODO:
                // SAFETY:
                handle: unsafe { Internal::new(self.height, self.node) },
            }
        }
    }
}

pub struct DynNodeMut<'a, T, const B: usize, const C: usize> {
    height: usize,
    pub(crate) node: &'a mut Node<T, B, C>,
}

impl<'a, T, const B: usize, const C: usize> DynNodeMut<'a, T, B, C> {
    pub unsafe fn new(height: usize, node: &'a mut Node<T, B, C>) -> Self {
        Self { height, node }
    }

    pub const fn len(&self) -> usize {
        self.node.len()
    }

    pub fn node_ptr_mut(&mut self) -> *mut Node<T, B, C> {
        self.node
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn into_variant_mut(self) -> VariantMut<'a, T, B, C> {
        if self.height == 0 {
            VariantMut::Leaf {
                // SAFETY: the safety invariant `self.len() <= C` is satisfied.
                handle: unsafe { LeafMut::new(self.node) },
            }
        } else {
            VariantMut::Internal {
                // SAFETY: the safety invariant `self.len() > C` is satisfied.
                handle: unsafe { InternalMut::new(self.height, self.node) },
            }
        }
    }
}

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
    pub(crate) length: NonZeroUsize,
    pub(crate) ptr: NodePtr<T, B, C>,
}

pub union NodePtr<T, const B: usize, const C: usize> {
    pub(crate) children: NonNull<[Option<Node<T, B, C>>; B]>,
    pub(crate) values: NonNull<[MaybeUninit<T>; C]>,
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

    pub fn set_length(&mut self, length: usize) {
        self.length = NonZeroUsize::new(length).unwrap();
    }

    fn from_children(length: usize, children: Box<[Option<Self>; B]>) -> Self {
        assert!(length > C);

        // SAFETY: `length > C`, so the `Node` is considered an internal node as it should be.
        Self {
            length: NonZeroUsize::new(length).unwrap(),
            ptr: NodePtr {
                children: unsafe { NonNull::new_unchecked(Box::into_raw(children)) },
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
                values: unsafe { NonNull::new_unchecked(Box::into_raw(values)) },
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
                values: unsafe { NonNull::new_unchecked(Box::into_raw(boxed_values)) },
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
