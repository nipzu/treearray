use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZeroUsize;
use core::{ptr, slice};

use alloc::boxed::Box;

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
    Internal {
        children: &'a [Option<Node<T, B, C>>; B],
    },
    Leaf {
        values: &'a [T],
    },
}

pub enum NodeVariantMut<'a, T, const B: usize, const C: usize> {
    Internal {
        children: &'a mut [Option<Node<T, B, C>>; B],
    },
    Leaf {
        values: &'a mut [T],
    },
}

enum InsertTo {
    Left(usize),
    Right(usize),
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
            NodeVariant::Leaf { values } => values.len() == C,
            NodeVariant::Internal { children } => matches!(children.last(), Some(&Some(_))),
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

    fn split(&mut self, index: usize) -> Self {
        let len = self.len();
        match self.variant_mut() {
            NodeVariantMut::Internal { children } => {
                let left_len = children[..index]
                    .iter()
                    .filter_map(|maybe_child| maybe_child.as_ref().map(Self::len))
                    .sum();
                let right_len = len - left_len;
                let tail_len = children.len() - index;
                let mut new_box = Box::new([Self::NONE; B]);
                children[index..].swap_with_slice(&mut new_box[..tail_len]);
                self.length = NonZeroUsize::new(left_len).unwrap();
                Self::from_children(right_len, new_box)
            }
            NodeVariantMut::Leaf { values } => {
                let tail_len = values.len() - index;
                let mut new_box = Box::new([Self::UNINIT; C]);
                unsafe {
                    let index_ptr = values.as_ptr().add(index);
                    let box_ptr = new_box.as_mut_ptr();
                    ptr::copy_nonoverlapping(index_ptr, box_ptr.cast::<T>(), tail_len);
                }
                self.length = NonZeroUsize::new(index).unwrap();
                unsafe { Self::from_values(tail_len, new_box) }
            }
        }
    }

    fn insert_fitting(&mut self, index: usize, value: T) {
        match self.variant_mut() {
            NodeVariantMut::Internal { children } => {
                let insert_index = find_insert_index(children, index);
                if let Some(new_child) = children[insert_index]
                    .as_mut()
                    .unwrap()
                    .insert(index, value)
                {
                    assert!(index < children.len());
                    unsafe {
                        let index_ptr = children.as_mut_ptr().add(index);
                        ptr::copy(index_ptr, index_ptr.add(1), children.len() - 1 - index);
                        ptr::write(index_ptr, Some(new_child));
                    }
                }
            }
            NodeVariantMut::Leaf { values } => {
                assert!(index < values.len());
                unsafe {
                    let index_ptr = values.as_mut_ptr().add(index);
                    ptr::copy(index_ptr, index_ptr.add(1), values.len() - index);
                    ptr::write(index_ptr, value);
                }
            }
        }

        // TODO: integer overflow
        self.length = NonZeroUsize::new(self.len().wrapping_add(1)).expect("length overflow");
    }

    fn insert_value(&mut self, index: usize, value: T) {
        match self.variant_mut() {
            NodeVariantMut::Internal { children } => {
                let insert_index = find_insert_index(children, index);
                if let Some(new_child) = children[insert_index]
                    .as_mut()
                    .unwrap()
                    .insert(index, value)
                {
                    assert!(index < children.len());
                    unsafe {
                        let index_ptr = children.as_mut_ptr().add(index);
                        ptr::copy(index_ptr, index_ptr.add(1), children.len() - 1 - index);
                        ptr::write(index_ptr, Some(new_child));
                    }
                }
            }
            NodeVariantMut::Leaf { values } => {
                assert!(index < values.len());
                unsafe {
                    let index_ptr = values.as_mut_ptr().add(index);
                    ptr::copy(index_ptr, index_ptr.add(1), values.len() - index);
                    ptr::write(index_ptr, value);
                }
            }
        }

        // TODO: integer overflow
        self.length = NonZeroUsize::new(self.len().wrapping_add(1)).expect("length overflow");
    }

    fn get_split(&self, index: usize) -> (usize, InsertTo) {
        let (insert_index, mid) = match self.variant() {
            NodeVariant::Internal { children } => (find_insert_index(children, index), B / 2),
            NodeVariant::Leaf { .. } => (index, C / 2),
        };

        if insert_index <= mid {
            (mid, InsertTo::Left(insert_index))
        } else {
            (mid + 1, InsertTo::Right(insert_index - mid))
        }
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<Self> {
        match self.variant_mut() {
            NodeVariantMut::Internal { children } => {
                let i = find_insert_index(children, index);
                if let Some(new_child) = children[i].as_mut().unwrap().insert(index, value) {
                    if self.is_full() {
                        self.split_and_insert_node(i, new_child);
                    } else {
                        self.insert_node(i, new_child);
                    }
                }
            },
            NodeVariantMut::Leaf { values } => {
                if self.is_full() {
                    self.split_and_insert_value(index, value);
                } else {
                    self.insert_value(index, value);
                }
            },
        };

        if self.is_full() {
            let (split_index, insert_spot) = self.get_split(index);
            let mut right = self.split(split_index);
            match insert_spot {
                InsertTo::Left(insert_index) => {
                    self.insert_fitting(insert_index, value);
                }
                InsertTo::Right(insert_index) => {
                    right.insert_fitting(insert_index, value);
                }
            }
            Some(right)
        } else {
            self.insert_fitting(index, value);
            None
        }
    }

    pub fn variant(&self) -> NodeVariant<T, B, C> {
        unsafe {
            if self.len() <= C {
                let values_ptr = (*self.inner.values).as_ptr().cast::<T>();
                let initialized_values = slice::from_raw_parts(values_ptr, self.len());
                NodeVariant::Leaf {
                    values: initialized_values,
                }
            } else {
                NodeVariant::Internal {
                    children: &self.inner.children,
                }
            }
        }
    }

    pub fn variant_mut(&mut self) -> NodeVariantMut<T, B, C> {
        unsafe {
            if self.len() <= C {
                let values_ptr = (*self.inner.values).as_mut_ptr().cast::<T>();
                let initialized_values = slice::from_raw_parts_mut(values_ptr, self.len());
                NodeVariantMut::Leaf {
                    values: initialized_values,
                }
            } else {
                NodeVariantMut::Internal {
                    children: &mut self.inner.children,
                }
            }
        }
    }
}

fn find_insert_index<T, const B: usize, const C: usize>(
    children: &[Option<Node<T, B, C>>; B],
    mut index: usize,
) -> usize {
    for (i, maybe_child) in children.iter().enumerate() {
        if let Some(child) = maybe_child {
            if index <= child.len() {
                return i;
            }
            index -= child.len();
        }
    }
    unreachable!();
}

impl<T, const B: usize, const C: usize> Drop for Node<T, B, C> {
    fn drop(&mut self) {
        unsafe {
            match self.variant_mut() {
                NodeVariantMut::Leaf { values } => {
                    ptr::drop_in_place(values);
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
