use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZeroUsize;
use core::ptr;

use alloc::boxed::Box;

use crate::panics::panic_length_overflow;

mod handle;

use handle::{InternalHandle, InternalHandleMut, LeafHandle, LeafHandleMut};

pub struct Node<T, const B: usize, const C: usize> {
    // Invariant: `length` is the number of values that this node eventually has as children
    //
    // If `self.length <= C`, this node is a leaf node with
    // exactly `size` initialized values held in `self.inner.values`.
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

    fn is_full(&self) -> bool {
        match self.variant() {
            NodeVariant::Leaf { handle } => handle.values().len() == C,
            NodeVariant::Internal { handle } => matches!(handle.children().last(), Some(&Some(_))),
        }
    }

    fn from_children(length: usize, children: Box<[Option<Self>; B]>) -> Self {
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

    unsafe fn from_values(length: usize, values: Box<[MaybeUninit<T>; C]>) -> Self {
        debug_assert!(length <= C);
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
        Self {
            length: NonZeroUsize::new(1).unwrap(),
            inner: NodeInner {
                values: ManuallyDrop::new(boxed_values),
            },
        }
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Self> {
        match self.variant_mut() {
            NodeVariantMut::Internal { mut handle } => {
                if let Some((new_child, insert_index)) = handle.insert(index, value) {
                    unsafe {
                        if handle.is_full() {
                            return Some(handle.split_and_insert_node(insert_index, new_child));
                        }

                        handle.insert_fitting(insert_index, new_child);
                    }
                }
                self.length = NonZeroUsize::new(self.len() + 1).unwrap();
            }

            NodeVariantMut::Leaf { mut handle } => unsafe {
                if handle.is_full() {
                    return Some(handle.split_and_insert_value(index, value));
                }

                handle.insert_fitting(index, value);
                self.length = NonZeroUsize::new(self.len() + 1).unwrap();
            },
        }
        None
    }

    pub const fn variant(&self) -> NodeVariant<T, B, C> {
        unsafe {
            if self.len() <= C {
                NodeVariant::Leaf {
                    handle: LeafHandle::new(self),
                }
            } else {
                NodeVariant::Internal {
                    handle: InternalHandle::new(self),
                }
            }
        }
    }

    pub fn variant_mut(&mut self) -> NodeVariantMut<T, B, C> {
        unsafe {
            if self.len() <= C {
                NodeVariantMut::Leaf {
                    handle: LeafHandleMut::new(self),
                }
            } else {
                NodeVariantMut::Internal {
                    handle: InternalHandleMut::new(self),
                }
            }
        }
    }
}

impl<T, const B: usize, const C: usize> Drop for Node<T, B, C> {
    fn drop(&mut self) {
        unsafe {
            match self.variant_mut() {
                NodeVariantMut::Leaf { mut handle } => {
                    ptr::drop_in_place(handle.values_mut());
                    ManuallyDrop::drop(&mut self.inner.values);
                }
                NodeVariantMut::Internal { .. } => {
                    ManuallyDrop::drop(&mut self.inner.children);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_node_size() {
        use core::mem::size_of;

        assert_eq!(size_of::<Node<i32, 10, 32>>(), 2 * size_of::<usize>());
        assert_eq!(size_of::<Node<i128, 3, 3>>(), 2 * size_of::<usize>());
        assert_eq!(size_of::<Node<i64, 0, 0>>(), 2 * size_of::<usize>());

        assert_eq!(
            size_of::<Option<Node<i32, 10, 32>>>(),
            2 * size_of::<usize>()
        );
        assert_eq!(
            size_of::<Option<Node<i128, 3, 3>>>(),
            2 * size_of::<usize>()
        );
        assert_eq!(
            size_of::<Option<Node<u128, 0, 0>>>(),
            2 * size_of::<usize>()
        );
    }
}
