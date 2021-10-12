#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::num::NonZeroUsize;

use core::ops::{Index, IndexMut};

pub struct TreeArray<T> {
    root_node: Option<TreeArrayNode<T>>,
}

struct TreeArrayNode<T> {
    value: T,
    subtree_size: NonZeroUsize,
    left_subtree: Option<Box<TreeArrayNode<T>>>,
    right_subtree: Option<Box<TreeArrayNode<T>>>,
}

pub fn foo(x: &TreeArray<i32>) -> Vec<&i32> {
    use core::iter::FromIterator;
    Vec::from_iter(x.iter())
}

impl<T> TreeArray<T> {
    pub const fn new() -> Self {
        Self { root_node: None }
    }

    pub const fn len(&self) -> usize {
        if let Some(root) = &self.root_node {
            root.subtree_size.get()
        } else {
            0
        }
        // self.root_node.as_ref().map_or(0, |n| n.subtree_size)
    }

    pub const fn is_empty(&self) -> bool {
        self.root_node.is_none()
    }

    #[inline(never)]
    pub fn get(&self, mut index: usize) -> Option<&T> {
        // fast path for out of bounds access
        // after this, we can unwrap all options
        if index >= self.len() {
            return None;
        }

        let mut cur_node = self.root_node.as_ref().unwrap();

        // recursively search for the index in the tree
        //
        // `index` is updated such that it is always the offset
        // from the leftmost node of the current subtree
        loop {
            if let Some(node) = &cur_node.left_subtree {
                // continue to left subtree
                if index < node.subtree_size.get() {
                    cur_node = node;
                    continue;
                }

                // index was not in left subtree
                index -= cur_node.subtree_size.get();
            }

            // we have found the index
            if index == 0 {
                return Some(&cur_node.value);
            }

            // continue to right subtree
            index -= 1;
            cur_node = cur_node.right_subtree.as_ref().unwrap();
        }
    }

    #[inline(never)]
    pub fn get_mut(&mut self, mut index: usize) -> Option<&mut T> {
        // fast path for out of bounds access
        // after this, we can unwrap all options
        if index >= self.len() {
            return None;
        }

        let mut cur_node = self.root_node.as_mut().unwrap();

        // recursively search for the index in the tree
        //
        // `index` is updated such that it is always the offset
        // from the leftmost node of the current subtree
        loop {
            if let Some(node) = &mut cur_node.left_subtree {
                // continue to left subtree
                if index < node.subtree_size.get() {
                    cur_node = node;
                    continue;
                }

                // index was not in left subtree
                index -= cur_node.subtree_size.get();
            }

            // we have found the index
            if index == 0 {
                return Some(&mut cur_node.value);
            }

            // continue to right subtree
            index -= 1;
            cur_node = cur_node.right_subtree.as_mut().unwrap();
        }
    }

    pub fn first(&self) -> Option<&T> {
        let mut cur_node = self.root_node.as_ref()?;

        while let Some(node) = &cur_node.left_subtree {
            cur_node = node;
        }

        Some(&cur_node.value)
    }

    pub fn first_mut(&mut self) -> Option<&mut T> {
        let mut cur_node = self.root_node.as_mut()?;

        while let Some(node) = &mut cur_node.left_subtree {
            cur_node = node;
        }

        Some(&mut cur_node.value)
    }

    pub fn last(&self) -> Option<&T> {
        let mut cur_node = self.root_node.as_ref()?;

        while let Some(node) = &cur_node.right_subtree {
            cur_node = node;
        }

        Some(&cur_node.value)
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        let mut cur_node = self.root_node.as_mut()?;

        while let Some(node) = &mut cur_node.right_subtree {
            cur_node = node;
        }

        Some(&mut cur_node.value)
    }

    pub fn clear(&mut self) {
        self.root_node = None;
    }

    pub fn insert(&mut self, index: usize, element: T) {
        // fast path for out of bounds access
        // after this, we can unwrap all options
        if index >= self.len() {
            panic!(
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
            )
        }

        let mut cur_node = self.root_node.as_mut().unwrap();

        // recursively search for the index in the tree
        //
        // `index` is updated such that it is always the offset
        // from the leftmost node of the current subtree
        loop {
            if let Some(node) = &mut cur_node.left_subtree {
                // continue to left subtree
                if index < node.subtree_size.get() {
                    cur_node = node;
                    continue;
                }

                // index was not in left subtree
                index -= cur_node.subtree_size.get();
            }

            // we have found the index
            if index == 0 {
                return Some(&mut cur_node.value);
            }

            // continue to right subtree
            index -= 1;
            cur_node = cur_node.right_subtree.as_mut().unwrap();
        }
    }

    pub fn iter(&self) -> Iter<T> {
        let mut path = Vec::new();

        if let Some(node) = &self.root_node {
            path.push(node);
            while let Some(node) = &path.last().unwrap().left_subtree {
                path.push(node);
            }
        }

        Iter { path }
    }
}

impl<T> Default for TreeArray<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Index<usize> for TreeArray<T> {
    type Output = T;
    #[track_caller]
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "index out of bounds: the len is {} but the index is {}",
                self.len(),
                index
            )
        })
    }
}

impl<T> IndexMut<usize> for TreeArray<T> {
    #[track_caller]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let l = self.len();
        self.get_mut(index).unwrap_or_else(|| {
            panic!(
                "index out of bounds: the len is {} but the index is {}",
                l, index
            )
        })
    }
}

pub struct IntoIter<T> {
    path: Vec<TreeArrayNode<T>>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut cur_node = self.path.pop()?;

        let value = cur_node.value;

        Some(value)
    }
}

impl<T> IntoIterator for TreeArray<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}

pub struct Iter<'a, T> {
    // a node in this vec means that we are in
    // the left left subtree of that node
    path: Vec<&'a TreeArrayNode<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let next_value = &self.path.last()?.value;

        if let Some(node) = &self.path.last().unwrap().right_subtree {
            self.path.push(node);
            while let Some(node) = &self.path.last().unwrap().left_subtree {
                self.path.push(node);
            }
        } else {
            self.path.pop().unwrap();
        }

        Some(next_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let v = TreeArray::<i32>::new();
        assert_eq!(v.len(), 0);
        assert!(v.is_empty());
    }

    #[test]
    fn test_get() {
        let v = TreeArray::<i32>::new();
        assert!(v.get(0).is_none());
        assert!(v.get(1).is_none());
    }
}
