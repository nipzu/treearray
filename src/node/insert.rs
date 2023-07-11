use super::handle::LeafMut;
use super::RawNodeWithLen;

use super::{InternalNode, NodeBase};

impl<'a, T: 'a> LeafMut<'a, T> {
    pub fn insert_value(&mut self, index: usize, value: T) -> Option<RawNodeWithLen<T>> {
        assert!(index <= self.len());

        if let Some(mut new_sibling_node) = self.split_if_full() {
            if index <= self.len() {
                self.values_mut().insert(index, value);
            } else {
                unsafe {
                    LeafMut::new(new_sibling_node.1.leaf)
                        .values_mut()
                        .insert(index - self.len(), value)
                };
                new_sibling_node.0 += 1;
            }

            Some(new_sibling_node)
        } else {
            self.values_mut().insert(index, value);
            None
        }
    }

    fn split_if_full(&mut self) -> Option<RawNodeWithLen<T>> {
        self.is_full().then(|| {
            let mut new_node = NodeBase::new_leaf();
            let mut new_leaf = unsafe { LeafMut::new(new_node.leaf) };
            self.values_mut().split(new_leaf.values_mut());

            unsafe {
                let old_next = (*self.node.as_ptr()).next;
                (*self.node.as_ptr()).next = Some(new_node.leaf);
                new_node.leaf.as_mut().next = old_next;
                new_node.leaf.as_mut().prev = Some(self.node);
                if let Some(next_of_next) = old_next {
                    (*next_of_next.as_ptr()).prev = Some(new_node.leaf);
                }
            };

            RawNodeWithLen(new_leaf.len(), new_node)
        })
    }
}

impl<T> InternalNode<T> {
    unsafe fn insert_length(&mut self, index: usize, len: usize) {
        let len_children = self.len_children();
        self.lengths.with_flat_lens(|lens| {
            for i in (index..len_children).rev() {
                lens[i + 1] = lens[i];
            }
            lens[index] = len;
        });
    }

    pub unsafe fn insert_child(
        &mut self,
        index: usize,
        node: RawNodeWithLen<T>,
    ) -> Option<RawNodeWithLen<T>> {
        unsafe {
            if let Some(mut new_next_sibling) = self.split_if_full() {
                if index <= usize::from(self.children_len) {
                    self.insert_fitting(index, node);
                } else {
                    let n = new_next_sibling.1.internal_mut();
                    n.insert_fitting(index - usize::from(self.children_len), node);
                    new_next_sibling.0 = n.len();
                }
                Some(new_next_sibling)
            } else {
                self.insert_fitting(index, node);
                None
            }
        }
    }

    unsafe fn insert_fitting(&mut self, index: usize, node: RawNodeWithLen<T>) {
        debug_assert!(!self.is_full());
        debug_assert!(index <= usize::from(self.children_len));
        unsafe {
            self.insert_length(index, node.0);
        }
        self.children().insert(index, node.1);
    }

    fn split_if_full(&mut self) -> Option<RawNodeWithLen<T>> {
        self.is_full().then(|| {
            let mut new_sibling_node = InternalNode::<T>::new();
            let new_sibling = unsafe { new_sibling_node.internal_mut() };

            new_sibling.lengths = self.lengths.split();
            self.children().split(new_sibling.children());

            RawNodeWithLen(new_sibling.len(), new_sibling_node)
        })
    }
}
